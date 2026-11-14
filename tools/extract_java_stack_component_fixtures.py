#!/usr/bin/env python3
"""Extract the Java 26.3 item-stack component facts VoxelCraft's stack slots model.

Reads only the reference: registry data and item tags from the official
server jar, and constants from the decompiled Java source (Vineflower output
of the same jar). Writes normalized names and numbers, no assets.

- `trim_pattern/*.json`, `trim_material/*.json` and `banner_pattern/*.json`:
  the registry names an armor trim or banner layer can name.
- `tags/item/cauldron_can_remove_dye.json`: the dyeable items (the items a
  water cauldron washes `dyed_color` off).
- `tags/item/trimmable_armor.json`, expanded through its nested tags.
- `DyeColor`: each dye's `textureDiffuseColor`, the colour `DyedItemColor`
  mixes.
- `WritableBookContent` / `WrittenBookContent`: page, title and generation
  limits.
- `DataComponents`: that each component id the stack slots model is a
  registered component type.
"""
import argparse
import hashlib
import io
import json
import re
from pathlib import Path
import zipfile

from extract_java_item_fixtures import SERVER_SHA1

MODELLED_COMPONENTS = [
    "banner_patterns",
    "base_color",
    "bundle_contents",
    "dyed_color",
    "trim",
    "writable_book_content",
    "written_book_content",
]


def registry_names(jar, folder):
    prefix = f"data/minecraft/{folder}/"
    return sorted(n[len(prefix):-len(".json")] for n in jar.namelist()
                  if n.startswith(prefix) and n.endswith(".json") and "/" not in n[len(prefix):])


def tag(jar, name, seen=()):
    if name in seen:
        raise ValueError(f"tag cycle at {name}")
    values = json.loads(jar.read(f"data/minecraft/tags/item/{name}.json"))["values"]
    out = set()
    for v in values:
        if isinstance(v, dict):
            raise ValueError(f"optional tag entry in {name}")
        if v.startswith("#minecraft:"):
            out |= tag(jar, v[len("#minecraft:"):], seen + (name,))
        elif v.startswith("minecraft:"):
            out.add(v[len("minecraft:"):])
        else:
            raise ValueError(f"unexpected tag entry {v}")
    return out


def constant(source, cls, name):
    text = (source / cls).read_text()
    found = re.findall(rf"public static final int {name} = (-?\d+);", text)
    if len(found) != 1:
        raise ValueError(f"{cls}: {name} not found once")
    return int(found[0])


def dye_colors(source):
    text = (source / "world/item/DyeColor.java").read_text()
    rows = re.findall(r"^\s+([A-Z_]+)\((\d+), \"(\w+)\", (\d+), MapColor", text, re.M)
    if len(rows) != 16 or [int(r[1]) for r in rows] != list(range(16)):
        raise ValueError("DyeColor constants changed shape")
    if "this.textureDiffuseColor = ARGB.opaque(textureDiffuseColor);" not in text:
        raise ValueError("DyeColor diffuse colour assignment changed")
    return {name: f"#{int(rgb):06X}" for _, _, name, rgb in rows}


def components(source):
    text = (source / "core/component/DataComponents.java").read_text()
    registered = set(re.findall(r'register\(\s*"([a-z_]+)"', text))
    missing = [c for c in MODELLED_COMPONENTS if c not in registered]
    if missing:
        raise ValueError(f"not registered data components: {missing}")
    return MODELLED_COMPONENTS


def extract(server, source):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        patterns = {n: json.loads(jar.read(f"data/minecraft/trim_pattern/{n}.json"))
                    for n in registry_names(jar, "trim_pattern")}
        trim_patterns = {n: {"asset_id": v["asset_id"], "decal": v["decal"]} for n, v in patterns.items()}
        trim_materials = registry_names(jar, "trim_material")
        banner_patterns = registry_names(jar, "banner_pattern")
        dyeable = sorted(tag(jar, "cauldron_can_remove_dye"))
        trimmable = sorted(tag(jar, "trimmable_armor"))
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "data/minecraft/trim_pattern/*.json",
            "data/minecraft/trim_material/*.json",
            "data/minecraft/banner_pattern/*.json",
            "data/minecraft/tags/item/cauldron_can_remove_dye.json",
            "data/minecraft/tags/item/trimmable_armor.json (nested tags expanded)",
            "net/minecraft/world/item/DyeColor.java (textureDiffuseColor)",
            "net/minecraft/world/item/component/WritableBookContent.java",
            "net/minecraft/world/item/component/WrittenBookContent.java",
            "net/minecraft/core/component/DataComponents.java",
        ],
        "components": components(source),
        "writable_book": {
            "max_pages": constant(source, "world/item/component/WritableBookContent.java", "MAX_PAGES"),
            "page_edit_length": constant(source, "world/item/component/WritableBookContent.java", "PAGE_EDIT_LENGTH"),
        },
        "written_book": {
            "page_length": constant(source, "world/item/component/WrittenBookContent.java", "PAGE_LENGTH"),
            "title_max_length": constant(source, "world/item/component/WrittenBookContent.java", "TITLE_MAX_LENGTH"),
            "max_generation": constant(source, "world/item/component/WrittenBookContent.java", "MAX_GENERATION"),
        },
        "dye_colors": dye_colors(source),
        "dyeable": dyeable,
        "trimmable_armor": trimmable,
        "trim_patterns": trim_patterns,
        "trim_materials": trim_materials,
        "banner_patterns": banner_patterns,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("server", type=Path, help="official Java 26.3 server.jar (the bundler)")
    parser.add_argument("source", type=Path, help="decompiled net/minecraft source folder of the same version")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server, args.source), indent=1) + "\n")


if __name__ == "__main__":
    main()
