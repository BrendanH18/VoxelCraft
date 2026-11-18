#!/usr/bin/env python3
"""Extract the v0.15 Track B workstation facts from the pinned Java 26.3 server.

Covers the banner pattern registry (`data/minecraft/banner_pattern`), the
`#no_item_required` and `pattern_item/*` banner pattern tags, the item tags
the workstations read (`loom_patterns`, `lectern_books`,
`book_cloning_target`, `clonable_maps`, `extendable_maps`), the stonecutter's
recipe list per input in recipe id order (`*_stonecutting.json`), the stack
sizes, rarities and glint of the spyglass, books and pattern items (`Items`),
the ominous banner's layers (`Raid.getBannerComponentPatch`), the loot table
entries and villager trades that give or want these items, and constants
(`SpyglassItem`, `BannerBlockEntity.MAX_PATTERNS`, `LecternBlock`,
`WritableBookContent`, `WrittenBookContent`).

It then runs a small independent model of
`LecternBlockEntity.getRedstoneSignal` and `setPage`'s clamp over a table of
page cases, written from the decompiled 26.3 classes. The game is compared
with the fixture in `tests/java_workstations.rs`.

Requires javap. Unknown bytecode forms fail rather than regenerating a
guessed fixture. No VoxelCraft source or existing fixture is consulted.
"""

import argparse
import hashlib
import io
import json
import math
import re
import subprocess
import tempfile
import zipfile
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

ITEMS = [
    "spyglass",
    "writable_book",
    "written_book",
    "flower_banner_pattern",
    "creeper_banner_pattern",
    "skull_banner_pattern",
    "mojang_banner_pattern",
    "globe_banner_pattern",
    "piglin_banner_pattern",
    "flow_banner_pattern",
    "guster_banner_pattern",
    "field_masoned_banner_pattern",
    "bordure_indented_banner_pattern",
]
ITEM_TAGS = ["loom_patterns", "lectern_books", "book_cloning_target", "clonable_maps", "extendable_maps"]
LOOT_TABLES = [
    "chests/bastion_other",
    "chests/trial_chambers/reward_unique",
    "chests/trial_chambers/reward_ominous_unique",
    "chests/abandoned_camp_common_chest",
]


def strip(name):
    if not name.startswith("minecraft:"):
        raise ValueError(f"not a minecraft id: {name}")
    return name[len("minecraft:") :]


def read_json(jar, path):
    return json.loads(jar.read(path))


def tag_values(jar, path):
    out = []
    for v in read_json(jar, path)["values"]:
        if not isinstance(v, str) or v.startswith("#"):
            raise ValueError(f"unexpected tag entry {v!r} in {path}")
        out.append(strip(v))
    return out


def item_window(items, field):
    """The `Items` static-initializer bytecode that registers `field`."""
    upper = field.upper()
    start = re.search(r"getstatic[^\n]*Field net/minecraft/references/ItemIds\." + upper + r":", items)
    if not start:
        raise ValueError(f"no registration for {field}")
    end = re.search(r"putstatic[^\n]*Field " + upper + r":", items[start.end() :])
    if not end:
        raise ValueError(f"no putstatic for {field}")
    return items[start.start() : start.end() + end.end()]


def item_facts(items, field, default_stack):
    window = item_window(items, field)
    sizes = re.findall(r"(?:iconst_(\d)|bipush\s+(\d+))\n[^\n]*Item\$Properties\.stacksTo", window)
    if len(sizes) > 1:
        raise ValueError(f"{field} calls stacksTo twice")
    stack = int(sizes[0][0] or sizes[0][1]) if sizes else default_stack
    if "stacksTo" in window and not sizes:
        raise ValueError(f"unrecognized stacksTo for {field}")
    rarities = re.findall(r"Field net/minecraft/world/item/Rarity\.(\w+):", window)
    if len(rarities) > 1:
        raise ValueError(f"{field} sets rarity twice")
    rarity = rarities[0].lower() if rarities else "common"
    glint = "ENCHANTMENT_GLINT_OVERRIDE" in window
    return {"max_stack": stack, "rarity": rarity, "glint": glint}


def int_constant(text, cls, name):
    m = re.search(r"static final (?:int|float) " + name + r" = ([0-9.]+)f?;", text)
    if not m:
        raise ValueError(f"missing {cls}.{name}")
    value = m[1]
    return float(value) if "." in value else int(value)


def ominous_layers(raid):
    """`Raid.getBannerComponentPatch`: `addIfRegistered(getter, pattern, colour)` calls in order."""
    body = re.search(r"getBannerComponentPatch\(.*?\n(.*?)\n\n", raid, re.S)
    if not body:
        raise ValueError("missing Raid.getBannerComponentPatch")
    calls = re.findall(
        r"getstatic[^\n]*Field net/minecraft/world/level/block/entity/BannerPatterns\.(\w+):[^\n]*\n"
        r"[^\n]*getstatic[^\n]*Field net/minecraft/world/item/DyeColor\.(\w+):[^\n]*\n"
        r"[^\n]*addIfRegistered",
        body[1],
    )
    if not calls:
        raise ValueError("unrecognized ominous banner layers")
    return calls


def pattern_keys(patterns_class):
    """`BannerPatterns` field name -> registry name."""
    out = {}
    for field, name in re.findall(
        r"ldc\s+#\d+\s+// String (\w+)\n[^\n]*create[^\n]*\n[^\n]*putstatic[^\n]*Field (\w+):", patterns_class
    ):
        out[name] = field
    if not out:
        raise ValueError("unrecognized BannerPatterns keys")
    return out


def lectern_model():
    """`getRedstoneSignal` and `setPage`'s clamp, from the decompiled class."""
    cases = []
    for count in [0, 1, 2, 3, 5, 15, 16, 100]:
        for page in sorted({0, 1, count // 2, count - 1, count, count + 3, -1}):
            clamped = min(max(page, 0), count - 1)
            shown = max(clamped, 0)
            progress = shown / (count - 1.0) if count > 1 else 1.0
            signal = math.floor(progress * 14.0) + 1
            cases.append({"pages": count, "page": page, "clamped": shown, "signal": signal})
    return cases


def loot_entries(jar, table):
    data = read_json(jar, f"data/minecraft/loot_table/{table}.json")
    out = []
    for pi, pool in enumerate(data["pools"]):
        total = sum(e.get("weight", 1) for e in pool["entries"])
        for e in pool["entries"]:
            name = strip(e.get("name", "minecraft:")) if e.get("name") else ""
            if name in ITEMS:
                out.append({"pool": pi, "item": name, "weight": e.get("weight", 1), "total_weight": total})
    return out


def trades(jar):
    out = []
    for path in sorted(n for n in jar.namelist() if n.startswith("data/minecraft/villager_trade/")):
        if not path.endswith(".json"):
            continue
        t = read_json(jar, path)
        gives, wants = strip(t["gives"]["id"]), strip(t["wants"]["id"])
        if gives in ITEMS or wants in ITEMS:
            parts = path[len("data/minecraft/villager_trade/") : -len(".json")].split("/")
            out.append(
                {
                    "profession": parts[0],
                    "level": int(parts[1]),
                    "gives": gives,
                    "gives_count": t["gives"].get("count", 1),
                    "wants": wants,
                    "wants_count": t["wants"].get("count", 1),
                    "max_uses": t["max_uses"],
                    "xp": t["xp"],
                }
            )
    return out


def stonecutter_menus(jar):
    menus = {}
    prefix = "data/minecraft/recipe/"
    for path in sorted(n for n in jar.namelist() if n.startswith(prefix) and n.endswith(".json")):
        r = read_json(jar, path)
        if r.get("type") != "minecraft:stonecutting":
            continue
        ingredient = r["ingredient"]
        if not isinstance(ingredient, str) or ingredient.startswith("#"):
            raise ValueError(f"{path}: unexpected ingredient {ingredient!r}")
        recipe_id = path[len(prefix) : -len(".json")]
        menus.setdefault(strip(ingredient), []).append(
            {"id": recipe_id, "result": strip(r["result"]["id"]), "count": r["result"].get("count", 1)}
        )
    # `RecipeManager` registers recipes in id order and `SingleInputSet`
    # keeps that order; the zip listing above is already sorted.
    for entries in menus.values():
        if [e["id"] for e in entries] != sorted(e["id"] for e in entries):
            raise ValueError("stonecutting recipes out of id order")
    return menus


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-workstations-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        items_class = inspect("world.item.Items")
        item_class = inspect("world.item.Item")
        default = re.search(r"public static final int DEFAULT_MAX_STACK_SIZE = (\d+);", item_class)
        if not default:
            raise ValueError("missing Item.DEFAULT_MAX_STACK_SIZE")
        spyglass = inspect("world.item.SpyglassItem")
        banner_entity = inspect("world.level.block.entity.BannerBlockEntity")
        lectern = inspect("world.level.block.LecternBlock")
        writable = inspect("world.item.component.WritableBookContent")
        written = inspect("world.item.component.WrittenBookContent")
        keys = pattern_keys(inspect("world.level.block.entity.BannerPatterns"))
        raid = inspect("world.entity.raid.Raid")
        layers = [{"pattern": keys[p], "color": c.lower()} for p, c in ominous_layers(raid)]

        with zipfile.ZipFile(path) as jar:
            patterns = sorted(
                n[len("data/minecraft/banner_pattern/") : -len(".json")]
                for n in jar.namelist()
                if n.startswith("data/minecraft/banner_pattern/") and n.endswith(".json")
            )
            for p in patterns:
                body = read_json(jar, f"data/minecraft/banner_pattern/{p}.json")
                if body["asset_id"] != f"minecraft:{p}":
                    raise ValueError(f"banner pattern {p} draws {body['asset_id']}")
            pattern_item_tags = {}
            prefix = "data/minecraft/tags/banner_pattern/pattern_item/"
            for n in sorted(jar.namelist()):
                if n.startswith(prefix) and n.endswith(".json"):
                    pattern_item_tags[n[len(prefix) : -len(".json")]] = tag_values(jar, n)
            facts = {
                "reference": "Java 26.3 behaviour data only; no artwork",
                "source": "official 26.3 server.jar via tools/extract_java_workstation_fixtures.py",
                "server_sha1": SERVER_SHA1,
                "banner_patterns": patterns,
                "no_item_required": tag_values(jar, "data/minecraft/tags/banner_pattern/no_item_required.json"),
                "pattern_item_tags": pattern_item_tags,
                "item_tags": {t: tag_values(jar, f"data/minecraft/tags/item/{t}.json") for t in ITEM_TAGS},
                "items": {i: item_facts(items_class, i, int(default[1])) for i in ITEMS},
                "ominous_banner": {"base": "white", "layers": layers},
                "constants": {
                    "SpyglassItem.USE_DURATION": int_constant(spyglass, "SpyglassItem", "USE_DURATION"),
                    "SpyglassItem.ZOOM_FOV_MODIFIER": int_constant(spyglass, "SpyglassItem", "ZOOM_FOV_MODIFIER"),
                    "BannerBlockEntity.MAX_PATTERNS": int_constant(banner_entity, "BannerBlockEntity", "MAX_PATTERNS"),
                    "LecternBlock.PAGE_CHANGE_IMPULSE_TICKS": int_constant(
                        lectern, "LecternBlock", "PAGE_CHANGE_IMPULSE_TICKS"
                    ),
                    "WritableBookContent.MAX_PAGES": int_constant(writable, "WritableBookContent", "MAX_PAGES"),
                    "WritableBookContent.PAGE_EDIT_LENGTH": int_constant(
                        writable, "WritableBookContent", "PAGE_EDIT_LENGTH"
                    ),
                    "WrittenBookContent.TITLE_MAX_LENGTH": int_constant(
                        written, "WrittenBookContent", "TITLE_MAX_LENGTH"
                    ),
                    "WrittenBookContent.MAX_GENERATION": int_constant(written, "WrittenBookContent", "MAX_GENERATION"),
                },
                "stonecutter_menus": stonecutter_menus(jar),
                "loot": {t: loot_entries(jar, t) for t in LOOT_TABLES},
                "trades": trades(jar),
                "lectern_signal": lectern_model(),
            }
    return facts


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("server", type=Path, help="the official 26.3 server.jar (bundler)")
    parser.add_argument("output", type=Path, help="where to write workstations.json")
    args = parser.parse_args()
    facts = extract(args.server)
    args.output.write_text(json.dumps(facts, indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
