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

v0.17 Track G adds the core workstations under `furnace`, `brewing`,
`enchanting`, `anvil`, `grindstone`, `smithing` and `comparator`: the
constants of `AbstractFurnaceBlockEntity`, `BrewingStandBlockEntity`,
`AnvilMenu`, `AnvilBlock` and `FallingBlockEntity` (with checks on the
bytecode that uses them), the `cooking/*` and `brewing/*` context providers,
the `#block/fast_cooking` predicate, every cooking fuel and brewing fuel,
the cooking and brewing recipes, the enchanting table's bookshelf tags, and
the smithing recipes' slot items. `java_workstation_models.py` then plays
furnace and brewing stand scenarios tick by tick and computes enchanting
offers (seed, bookshelves and item to costs, clues and rolls), anvil and
grindstone results, falling-anvil wear and comparator output in Java's
int, float and random arithmetic. The game is compared with these in
`tests/workstations.rs`.

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

import extract_java_equipment_fixtures
import extract_java_repair_fixtures
import java_workstation_models as models
from extract_java_enchantment_fixtures import cost as enchantment_cost_form
from extract_java_enchantment_fixtures import expand_enchantments, expand_items
from extract_java_item_fixtures import SERVER_SHA1
from java_item_associations import provider_ids

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


# v0.17 Track G: furnaces, brewing stands, enchanting tables, anvils,
# grindstones and smithing tables.

OPERAND = re.compile(
    r"\b(?:(iconst_m1)|iconst_(\d)|[bs]ipush\s+(-?\d+)|fconst_(\d)|dconst_(\d)|lconst_(\d)"
    r"|ldc\w*\s+#\d+\s+// (?:int|float|double|long) (-?[0-9.]+(?:E-?[0-9]+)?)[dfl]?)"
)
COOKING_TYPES = {"smelting": "furnace", "smoking": "smoker", "blasting": "blast_furnace"}
VECTOR_ITEMS = [
    "book",
    "bow",
    "crossbow",
    "trident",
    "fishing_rod",
    "mace",
    "diamond_sword",
    "iron_pickaxe",
    "golden_axe",
    "wooden_shovel",
    "netherite_hoe",
    "golden_helmet",
    "diamond_chestplate",
    "leather_boots",
    "turtle_helmet",
    "chainmail_leggings",
]
ANVIL_ITEMS = [
    "iron_pickaxe",
    "diamond_sword",
    "bow",
    "stick",
    "iron_ingot",
    "diamond",
    "book",
    "enchanted_book",
]
STACK_ITEMS = [
    "raw_iron",
    "iron_ore",
    "iron_ingot",
    "coal",
    "beef",
    "cooked_beef",
    "sand",
    "red_sand",
    "glass",
    "wet_sponge",
    "sponge",
    "bucket",
    "water_bucket",
    "lava_bucket",
    "stick",
    "dried_kelp_block",
    "raw_gold",
    "gold_ingot",
    "potion",
    "glass_bottle",
    "blaze_powder",
    "sugar",
    "nether_wart",
    "spider_eye",
    "dragon_breath",
    "redstone",
]


def method(text, signature):
    """The bytecode of the method whose declaration contains `signature`."""
    start = text.find(signature)
    if start < 0:
        raise ValueError(f"missing method {signature}")
    end = text.find("\n\n", start)
    return text[start : end if end > 0 else len(text)]


def operands(body):
    """The numeric constants a method body pushes, in order."""
    out = []
    for m in OPERAND.finditer(body):
        if m[1]:
            out.append(-1)
        elif m[2] or m[3] or m[6]:
            out.append(int(m[2] or m[3] or m[6]))
        elif m[4] or m[5]:
            out.append(float(m[4] or m[5]))
        else:
            out.append(float(m[7]) if "." in m[7] or "E" in m[7] else int(m[7]))
    return out


def expect(values, wanted, what):
    if values != wanted:
        raise ValueError(f"unrecognized {what}: {values}")
    return values


def static(text, name):
    m = re.search(r"static final (?:int|float|double|long) " + name + r" = (-?[0-9.]+(?:E-?[0-9]+)?)[fdL]?;", text)
    if not m:
        raise ValueError(f"missing {name}")
    return float(m[1]) if "." in m[1] or "E" in m[1] else int(m[1])


def requires(body, needles, what):
    for needle in needles:
        if needle not in body:
            raise ValueError(f"{what} no longer uses {needle}")


def holder_items(jar, value):
    """An item holder set: a tag, an id or a list of ids."""
    if isinstance(value, list):
        return sorted({x for v in value for x in holder_items(jar, v)})
    if value.startswith("#minecraft:"):
        return expand_items(jar, value[len("#minecraft:") :])
    return [strip(value)]


def expand_block_tag(jar, name):
    out = set()
    for v in read_json(jar, f"data/minecraft/tags/block/{name}.json")["values"]:
        out |= expand_block_tag(jar, v[len("#minecraft:") :]) if v.startswith("#") else {strip(v)}
    return out


def window_or_block(items_class, name):
    """The `Items` static-initializer bytecode between the previous field
    store and the store of `name`'s field, whatever form registers it."""
    end = re.search(r"putstatic[^\n]*Field " + name.upper() + r":Lnet/minecraft/world/item/Item;", items_class)
    if not end:
        raise ValueError(f"no Items field for {name}")
    start = items_class.rfind("putstatic", 0, end.start())
    return items_class[start + len("putstatic") : end.end()]


def max_stack(items_class, name):
    window = window_or_block(items_class, name)
    sizes = re.findall(r"(?:iconst_(\d)|bipush\s+(\d+))\n[^\n]*Item\$Properties\.stacksTo", window)
    if "stacksTo" in window and len(sizes) != 1:
        raise ValueError(f"unrecognized stacksTo for {name}")
    return int(sizes[0][0] or sizes[0][1]) if sizes else 64


def remainder(items_class, name):
    m = re.search(
        r"Field (\w+):Lnet/minecraft/world/item/Item;\n[^\n]*craftRemainder", window_or_block(items_class, name)
    )
    return m[1].lower() if m else None


def window_int(window, call):
    """The constant pushed right before `Item$Properties.<call>(I)`."""
    m = re.findall(
        r"(?:iconst_(\d)|bipush\s+(\d+)|sipush\s+(\d+))\n[^\n]*Item\$Properties\." + call + r":\(I\)", window
    )
    if len(m) > 1:
        raise ValueError(f"{call} set twice")
    return int(next(x for x in m[0] if x)) if m else None


def tool_materials(inspect):
    """`ToolMaterial` name -> (durability, enchantability)."""
    block = inspect("world.item.ToolMaterial").split("  static {};")[1]
    out = {}
    for code, name in zip(
        *[iter(re.split(r"putstatic\s+#\d+\s+// Field (\w+):Lnet/minecraft/world/item/ToolMaterial;", block))] * 2
    ):
        ints = [v for v in operands(code) if isinstance(v, int)]
        if len(ints) != 2:
            raise ValueError(f"unrecognized ToolMaterial {name}: {ints}")
        out[name] = ints
    return out


def vector_items(server, inspect, items_class):
    """Durability and enchantability of the items the vectors use."""
    tools = tool_materials(inspect)
    equipment = extract_java_equipment_fixtures.extract(server)
    out = {}
    for name in sorted(set(VECTOR_ITEMS + ANVIL_ITEMS)):
        window = window_or_block(items_class, name)
        material = re.search(r"Field net/minecraft/world/item/ToolMaterial\.(\w+):", window)
        armor = equipment["items"].get(name)
        if material:
            durability, enchantable = tools[material[1]]
        elif armor:
            m = equipment["materials"][armor["material"]]
            durability = equipment["armor_types"][armor["type"]]["durability_unit"] * m["durability_multiplier"]
            enchantable = m["enchantability"]
        else:
            durability, enchantable = window_int(window, "durability"), window_int(window, "enchantable")
        out[name] = {"max_damage": durability, "enchantable": enchantable, "max_stack": max_stack(items_class, name)}
    return out


def enchantment_definitions(jar):
    known = sorted(
        n[len("data/minecraft/enchantment/") : -len(".json")]
        for n in jar.namelist()
        if n.startswith("data/minecraft/enchantment/") and n.endswith(".json")
    )
    defs = {}
    for e in known:
        d = read_json(jar, f"data/minecraft/enchantment/{e}.json")
        exclusive = d.get("exclusive_set", [])
        if isinstance(exclusive, str) and exclusive.startswith("#"):
            exclusive = expand_enchantments(jar, exclusive[len("#minecraft:") :], known)
        elif isinstance(exclusive, str):
            exclusive = [strip(exclusive)]
        else:
            exclusive = [strip(x) for x in exclusive]
        supported = holder_items(jar, d["supported_items"])
        defs[e] = {
            "weight": d["weight"],
            "max_level": d["max_level"],
            "min_cost": enchantment_cost_form(d["min_cost"], e),
            "max_cost": enchantment_cost_form(d["max_cost"], e),
            "anvil_cost": d["anvil_cost"],
            "supported": supported,
            "primary": holder_items(jar, d["primary_items"]) if "primary_items" in d else supported,
            "exclusive": exclusive,
        }
    return known, defs


def cooking_recipes(jar):
    """Recipe type -> input -> (id, result, experience, time)."""
    out = {kind: {} for kind in COOKING_TYPES}
    times = set()
    prefix = "data/minecraft/recipe/"
    for path in sorted(n for n in jar.namelist() if n.startswith(prefix) and n.endswith(".json")):
        r = read_json(jar, path)
        kind = r.get("type", "")[len("minecraft:") :]
        if kind not in COOKING_TYPES:
            continue
        rid = path[len(prefix) : -len(".json")]
        times.add((kind, r["cookingtime"]))
        for item in holder_items(jar, r["ingredient"]):
            if item in out[kind]:
                raise ValueError(f"two {kind} recipes take {item}")
            out[kind][item] = (rid, strip(r["result"]["id"]), r["experience"], r["cookingtime"])
    return out, sorted(times)


def brewing_recipes(jar):
    mixes = {}
    prefix = "data/minecraft/recipe/brewing/"
    for path in sorted(n for n in jar.namelist() if n.startswith(prefix) and n.endswith(".json")):
        r = read_json(jar, path)
        potion_in = r["input"].get("potion_contents", {}).get("potions")
        potion_out = r["output"].get("components", {}).get("minecraft:potion_contents", {}).get("potion")
        key = (strip(r["input"]["item"]), strip(potion_in) if potion_in else None, strip(r["reagent"]["item"]))
        mixes[key] = (strip(r["output"]["id"]), strip(potion_out) if potion_out else None)
    return mixes


def stack(item, count=1, damage=0, enchantments=None, repair_cost=0, name=None):
    return {
        "item": item,
        "count": count,
        "damage": damage,
        "enchantments": enchantments or {},
        "repair_cost": repair_cost,
        "name": name,
    }


def anvil_cases():
    s = stack
    book = "enchanted_book"
    return [
        ("repair with four ingots", s("iron_pickaxe", damage=200), s("iron_ingot", 4), None, False),
        ("repair with prior work", s("diamond_sword", damage=1000, repair_cost=3), s("diamond", 1), None, False),
        ("nothing to repair", s("iron_pickaxe"), s("iron_ingot", 3), None, False),
        (
            "merge two swords",
            s("diamond_sword", damage=1500, enchantments={"sharpness": 3, "unbreaking": 2}),
            s("diamond_sword", damage=1200, enchantments={"sharpness": 3, "looting": 1}),
            None,
            False,
        ),
        ("book onto a sword", s("diamond_sword"), s(book, enchantments={"sharpness": 5}), None, False),
        (
            "one clash, one fit",
            s("diamond_sword", enchantments={"sharpness": 2}),
            s(book, enchantments={"smite": 3, "unbreaking": 1}),
            None,
            False,
        ),
        (
            "only a clash",
            s("diamond_sword", enchantments={"sharpness": 2}),
            s(book, enchantments={"smite": 3}),
            None,
            False,
        ),
        (
            "two books",
            s(book, enchantments={"protection": 2}),
            s(book, enchantments={"protection": 2, "mending": 1}),
            None,
            False,
        ),
        (
            "clashing books",
            s(book, enchantments={"protection": 4}),
            s(book, enchantments={"fire_protection": 4}),
            None,
            False,
        ),
        ("a stack on the left", s("stick", 3), s(book, enchantments={"unbreaking": 1}), None, True),
        (
            "too expensive",
            s("diamond_sword", repair_cost=31, enchantments={"sharpness": 4}),
            s(book, repair_cost=15, enchantments={"sharpness": 4}),
            None,
            False,
        ),
        (
            "creative ignores too expensive",
            s("diamond_sword", repair_cost=31, enchantments={"sharpness": 4}),
            s(book, repair_cost=15, enchantments={"sharpness": 4}),
            None,
            True,
        ),
        ("rename", s("diamond_sword", repair_cost=7), None, "Excalibur", False),
        ("rename capped at 39", s("diamond_sword", repair_cost=63), None, "Old Sword", False),
        ("rename beside a spare", s("iron_pickaxe"), s("iron_pickaxe"), "Pick", False),
        ("clear a name", s("diamond_sword", repair_cost=1, name="Blade"), None, "", False),
        ("same name costs nothing", s("diamond_sword", name="Blade"), None, "Blade", False),
        ("rename while repairing", s("iron_pickaxe", damage=100), s("iron_ingot", 2), "Digger", False),
        ("a book keeps the name", s("diamond_sword", name="Blade"), s(book, enchantments={"looting": 2}), None, False),
        ("creative puts anything on", s("bow"), s(book, enchantments={"sharpness": 1}), None, True),
        ("survival can't", s("bow"), s(book, enchantments={"sharpness": 1}), None, False),
        (
            "capped at the maximum",
            s("diamond_sword", enchantments={"sharpness": 5}),
            s(book, enchantments={"sharpness": 5}),
            None,
            False,
        ),
        (
            "mending and infinity",
            s("bow", enchantments={"infinity": 1}),
            s(book, enchantments={"mending": 1}),
            None,
            False,
        ),
        (
            "prior work adds up",
            s("iron_pickaxe", repair_cost=15, enchantments={"efficiency": 3}),
            s(book, repair_cost=3, enchantments={"unbreaking": 3}),
            None,
            False,
        ),
    ]


def grindstone_cases():
    s = stack
    book = "enchanted_book"
    return [
        (
            "disenchant",
            s("diamond_sword", damage=100, repair_cost=7, enchantments={"sharpness": 3, "vanishing_curse": 1}),
            None,
        ),
        (
            "repair two pickaxes",
            s("iron_pickaxe", damage=200, enchantments={"efficiency": 2}),
            s("iron_pickaxe", damage=150, enchantments={"binding_curse": 1, "unbreaking": 3}),
        ),
        ("book to book", s(book, enchantments={"mending": 1}), None),
        ("cursed book stays", s(book, enchantments={"binding_curse": 1, "mending": 1}), None),
        ("two books", s(book, enchantments={"mending": 1}), s(book, enchantments={"mending": 1})),
        ("nothing to remove", s("iron_pickaxe"), None),
        ("different items", s("iron_pickaxe", damage=10), s("diamond_sword", damage=10)),
        ("plain repair", s("iron_pickaxe", damage=240), s("iron_pickaxe", damage=240)),
        ("a stack", s("stick", 2, enchantments={"unbreaking": 1}), None),
        (
            "keeps the first curse level",
            s("bow", damage=5, enchantments={"vanishing_curse": 1, "power": 4}),
            s("bow", damage=5, enchantments={"vanishing_curse": 1, "binding_curse": 1}),
        ),
        ("only the second slot", None, s("bow", enchantments={"power": 5, "flame": 1})),
    ]


def furnace_scenarios():
    """(name, block, ops). Ops: `["input", item, count]` (`setItem`),
    `["fuel", item, count]`, `["take_output"]` (a hopper's take) or
    `["tick", n]`, recording the state after each tick op."""
    return [
        (
            "coal smelts eight",
            "furnace",
            [
                ["input", "raw_iron", 9],
                ["fuel", "coal", 1],
                ["tick", 1],
                ["tick", 199],
                ["tick", 1],
                ["tick", 1399],
                ["tick", 1],
                ["tick", 5],
            ],
        ),
        (
            "smoker",
            "smoker",
            [["input", "beef", 10], ["fuel", "coal", 1], ["tick", 100], ["tick", 700], ["tick", 1], ["tick", 20]],
        ),
        (
            "blast furnace kelp",
            "blast_furnace",
            [["input", "raw_gold", 25], ["fuel", "dried_kelp_block", 1], ["tick", 2000], ["tick", 1], ["tick", 3]],
        ),
        (
            "a stick runs out mid-item",
            "furnace",
            [
                ["input", "raw_iron", 2],
                ["fuel", "stick", 1],
                ["tick", 100],
                ["tick", 1],
                ["tick", 10],
                ["fuel", "stick", 1],
                ["tick", 1],
                ["tick", 150],
            ],
        ),
        (
            "smoker relights part way",
            "smoker",
            [
                ["input", "beef", 3],
                ["fuel", "stick", 1],
                ["tick", 50],
                ["tick", 1],
                ["tick", 5],
                ["fuel", "stick", 1],
                ["tick", 1],
                ["tick", 60],
            ],
        ),
        (
            "fresh smoker rescales",
            "smoker",
            [["fuel", "bucket", 1], ["input", "beef", 1], ["tick", 3], ["fuel", "coal", 1], ["tick", 1], ["tick", 99]],
        ),
        (
            "lava leaves the bucket",
            "furnace",
            [["input", "sand", 64], ["fuel", "lava_bucket", 1], ["tick", 1], ["tick", 12799], ["tick", 1]],
        ),
        (
            "wet sponge fills a bucket",
            "furnace",
            [["input", "wet_sponge", 1], ["fuel", "coal", 1], ["tick", 1], ["fuel", "bucket", 1], ["tick", 199]],
        ),
        (
            "swap the input",
            "furnace",
            [
                ["input", "raw_iron", 1],
                ["fuel", "coal", 1],
                ["tick", 150],
                ["input", "iron_ore", 1],
                ["tick", 1],
                ["input", "iron_ore", 2],
                ["tick", 199],
            ],
        ),
        (
            "recipes tally apart",
            "furnace",
            [
                ["input", "iron_ore", 1],
                ["fuel", "coal", 2],
                ["tick", 200],
                ["input", "raw_iron", 2],
                ["tick", 400],
                ["take_output"],
                ["input", "sand", 3],
                ["tick", 600],
                ["input", "red_sand", 2],
                ["tick", 400],
            ],
        ),
        (
            "full output stops it",
            "furnace",
            [["input", "raw_iron", 2], ["fuel", "coal", 1], ["tick", 100], ["take_output"], ["tick", 1]],
        ),
    ]


def brewing_scenarios():
    water, awkward = ["potion", "water"], ["potion", "awkward"]
    return [
        (
            "awkward on one powder",
            [
                ["bottles", [water, water, None]],
                ["ingredient", "nether_wart", 2],
                ["fuel", "blaze_powder", 1],
                ["tick", 1],
                ["tick", 399],
                ["tick", 1],
                ["tick", 5],
            ],
        ),
        (
            "swap mid-brew",
            [
                ["bottles", [awkward, None, None]],
                ["ingredient", "sugar", 1],
                ["fuel", "blaze_powder", 1],
                ["tick", 100],
                ["ingredient", "spider_eye", 1],
                ["tick", 1],
                ["tick", 1],
                ["tick", 400],
            ],
        ),
        (
            "swap on the last tick",
            [
                ["bottles", [awkward, None, None]],
                ["ingredient", "sugar", 1],
                ["fuel", "blaze_powder", 1],
                ["tick", 400],
                ["ingredient", "spider_eye", 1],
                ["tick", 1],
            ],
        ),
        (
            "bottles taken away",
            [
                ["bottles", [awkward, None, None]],
                ["ingredient", "sugar", 1],
                ["fuel", "blaze_powder", 1],
                ["tick", 10],
                ["bottles", [None, None, None]],
                ["tick", 1],
                ["bottles", [awkward, None, None]],
                ["tick", 401],
            ],
        ),
        ("no fuel", [["bottles", [awkward, None, None]], ["ingredient", "sugar", 1], ["tick", 50]]),
        (
            "dragon's breath leaves bottles",
            [
                ["bottles", [["splash_potion", "swiftness"], None, None]],
                ["ingredient", "dragon_breath", 2],
                ["fuel", "blaze_powder", 1],
                ["tick", 401],
                ["bottles", [["splash_potion", "swiftness"], None, None]],
                ["tick", 401],
            ],
        ),
        (
            "twenty brews a powder",
            [
                ["bottles", [water, None, None]],
                ["ingredient", "redstone", 64],
                ["fuel", "blaze_powder", 2],
                ["tick", 1],
                ["tick", 1],
            ],
        ),
    ]


def comparator_cases():
    return {
        "furnace": [
            [None, None, None],
            [["raw_iron", 1], None, None],
            [["raw_iron", 64], ["coal", 64], ["iron_ingot", 64]],
            [["raw_iron", 32], ["lava_bucket", 1], None],
            [["raw_iron", 21], None, ["iron_ingot", 43]],
            [None, ["bucket", 1], None],
        ],
        "brewing": [
            [None, None, None, None, None],
            [["potion", 1], None, None, None, None],
            [["potion", 1], ["potion", 1], ["potion", 1], None, None],
            [["potion", 1], ["potion", 1], ["potion", 1], ["nether_wart", 64], ["blaze_powder", 64]],
            [None, None, None, ["sugar", 1], ["blaze_powder", 3]],
            [["glass_bottle", 1], None, None, None, None],
        ],
    }


def core_workstations(server, path, inspect, items_class, jar):
    furnace_class = inspect("world.level.block.entity.AbstractFurnaceBlockEntity")
    brewing_class = inspect("world.level.block.entity.BrewingStandBlockEntity")
    helper = inspect("world.item.enchantment.EnchantmentHelper")
    menu = inspect("world.inventory.EnchantmentMenu")
    table = inspect("world.level.block.EnchantingTableBlock")
    anvil_menu = inspect("world.inventory.AnvilMenu")
    anvil_block = inspect("world.level.block.AnvilBlock")
    falling = inspect("world.entity.item.FallingBlockEntity")
    grind_slot = inspect("world.inventory.GrindstoneMenu$4")
    grind = inspect("world.inventory.GrindstoneMenu")
    player = inspect("world.entity.player.Player")
    legacy = inspect("world.level.levelgen.LegacyRandomSource")
    bits = inspect("world.level.levelgen.BitRandomSource")
    container_menu = inspect("world.inventory.AbstractContainerMenu")
    mth = inspect("util.Mth")

    # Tripwires: the arithmetic the models copy.
    expect(
        operands(method(furnace_class, "public static void serverTick(")),
        [0, 1, 1, 1, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 0, 1, 0, 0, 0, 2, 0, 1],
        "AbstractFurnaceBlockEntity.serverTick",
    )
    requires(
        method(furnace_class, "public static void serverTick("),
        ["Math.ceil:(D)D", "consumeFuel", "getSpeedMultiplier", "Mth.clamp:(III)I"],
        "serverTick",
    )
    cook_time = method(
        furnace_class, "private static int getTotalCookTime(net.minecraft.world.item.crafting.RecipeHolder"
    )
    requires(cook_time, ["fdiv", "Math.ceil:(D)D"], "getTotalCookTime")
    requires(
        method(furnace_class, "private static void createExperience("),
        ["Mth.floor:(F)I", "Mth.frac:(F)F", "nextFloat"],
        "createExperience",
    )
    requires(method(furnace_class, "private static void burn("), ["WET_SPONGE", "BUCKET", "WATER_BUCKET"], "burn")
    expect(
        operands(method(brewing_class, "public static void serverTick(")),
        [4, 1, 4, 1, 0, 3, 1, 1, 0, 0, 0.0, 1.0, 1, 400.0, 0, 2],
        "BrewingStandBlockEntity.serverTick",
    )
    expect(
        operands(method(helper, "public static int getEnchantmentCost(")),
        [0, 15, 15, 8, 1, 1, 1, 3, 1, 1, 2, 3, 1, 2],
        "getEnchantmentCost",
    )
    select = method(
        helper,
        "public static java.util.List<net.minecraft.world.item.enchantment.EnchantmentInstance> selectEnchantment(",
    )
    expect(operands(select), [1, 4, 1, 4, 1, 1.0, 0.15, 1, 2147483647, 50, 2], "selectEnchantment")
    requires(
        select, ["Math.round:(F)I", "filterCompatibleEnchantments", "WeightedRandom.getRandomItem"], "selectEnchantment"
    )
    # The button's work happens in a lambda (`access.execute`).
    requires(menu, ["hasInfiniteMaterials", "onEnchantmentPerformed", "ItemStack.consume:"], "EnchantmentMenu")

    requires(method(player, "public void onEnchantmentPerformed("), ["nextInt:()I"], "onEnchantmentPerformed")
    expect(operands(method(table, "static {};"))[:6], [-2, 0, -2, 2, 1, 2], "EnchantingTableBlock.BOOKSHELF_OFFSETS")
    requires(
        method(table, "public static boolean isValidBookShelf("),
        ["ENCHANTMENT_POWER_PROVIDER", "ENCHANTMENT_POWER_TRANSMITTER", "idiv"],
        "isValidBookShelf",
    )
    expect(
        operands(method(anvil_menu, "public void createResult();")),
        [
            0,
            0,
            1,
            0,
            0,
            0,
            0,
            0,
            1,
            0,
            0,
            0,
            4,
            0,
            0,
            0,
            4,
            0,
            0,
            12,
            100,
            0,
            0,
            0,
            1,
            1,
            0,
            1,
            1,
            1,
            2,
            1,
            40,
            0,
            0,
            1,
            1,
            0,
            0,
            2147483647,
            40,
            39,
            1,
            40,
            0,
            0,
            0,
            0,
        ],
        "AnvilMenu.createResult",
    )
    expect(
        operands(method(anvil_menu, "public static int calculateIncreasedRepairCost(")),
        [2, 1, 2147483647],
        "calculateIncreasedRepairCost",
    )
    wear = re.findall(r"// float (0\.\d+)f", anvil_menu)
    expect(wear, ["0.12"], "AnvilMenu.onTake's wear chance")
    expect(operands(method(anvil_block, "protected void falling(")), [2.0, 40], "AnvilBlock.falling")
    expect(
        operands(method(falling, "public boolean causeFallDamage(")),
        [0, 1.0, 0, 0.0, 0.05, 0.05, 1, 0],
        "causeFallDamage",
    )
    expect(operands(method(grind_slot, "private int getExperienceAmount(")), [0, 0, 1, 2.0, 0], "getExperienceAmount")
    expect(
        operands(method(grind, "private net.minecraft.world.item.ItemStack mergeItems(")),
        [5, 100, 1, 2, 2, 0],
        "mergeItems",
    )
    expect(
        operands(method(legacy, "public void setSeed(")), [25214903917, 281474976710655], "LegacyRandomSource.setSeed"
    )
    expect(
        operands(method(legacy, "public int next(")), [25214903917, 11, 281474976710655, 48], "LegacyRandomSource.next"
    )
    if static(bits, "FLOAT_MULTIPLIER") != 5.9604645e-08:
        raise ValueError("BitRandomSource.FLOAT_MULTIPLIER changed")
    requires(
        method(container_menu, "public static int getRedstoneSignalFromContainer("),
        ["fdiv", "Mth.lerpDiscrete:(FII)I"],
        "getRedstoneSignalFromContainer",
    )
    requires(method(mth, "public static int lerpDiscrete("), ["fmul", "floor:(F)I"], "Mth.lerpDiscrete")

    # Furnaces.
    fast_blocks = [strip(b) for b in read_json(jar, "data/minecraft/predicate/block/fast_cooking.json")["blocks"]]
    provider = "data/minecraft/context_{}_provider/{}.json"
    speeds = {
        "normal": read_json(jar, provider.format("float", "cooking/normal_speed_multiplier")),
        "fast": read_json(jar, provider.format("float", "cooking/fast_speed_multiplier")),
    }
    reductions = {
        "normal": read_json(jar, provider.format("int", "cooking/normal_burn_time_reduction_factor")),
        "fast": read_json(jar, provider.format("int", "cooking/fast_burn_time_reduction_factor")),
    }
    speed_default = read_json(jar, provider.format("float", "cooking/speed_default"))
    if speed_default != {
        "type": "minecraft:conditional",
        "condition": "minecraft:block/fast_cooking",
        "on_false": "minecraft:cooking/normal_speed_multiplier",
        "on_true": "minecraft:cooking/fast_speed_multiplier",
    }:
        raise ValueError("cooking/speed_default changed")
    fuel_providers = provider_ids(path, "COOKING_TIME_")
    fuels = {}
    for item, name in sorted(fuel_providers.items()):
        body = read_json(jar, provider.format("int", f"cooking/time_{name.lower()}"))
        if body["type"] != "minecraft:div" or body["right"] != {
            "type": "minecraft:conditional",
            "condition": "minecraft:block/fast_cooking",
            "on_false": "minecraft:cooking/normal_burn_time_reduction_factor",
            "on_true": "minecraft:cooking/fast_burn_time_reduction_factor",
        }:
            raise ValueError(f"cooking/time_{name.lower()} changed form")
        fuels[item] = body["left"]
    recipes, recipe_times = cooking_recipes(jar)
    stacks = {name: max_stack(items_class, name) for name in STACK_ITEMS}
    remainders = {name: r for name in STACK_ITEMS if (r := remainder(items_class, name))}

    def furnace_run(block, ops):
        fast = block in fast_blocks
        kind = next(k for k, b in COOKING_TYPES.items() if b == block)
        reduction = reductions["fast" if fast else "normal"]
        f = models.Furnace(
            recipes[kind],
            {i: t // reduction for i, t in fuels.items()},
            lambda item: float(speeds["fast" if fast else "normal"]) if item in fuels else 1.0,
            stacks,
            remainders,
        )
        states = []
        for op in ops:
            if op[0] == "input":
                f.set_input((op[1], op[2]))
            elif op[0] == "fuel":
                f.fuel = (op[1], op[2])
            elif op[0] == "take_output":
                f.output = None
            else:
                for _ in range(op[1]):
                    f.tick()
                states.append(f.state())
        return states

    recipe_xp = {r[0]: r[2] for kind in recipes.values() for r in kind.values()}

    # Brewing stands.
    brewing_fuels = provider_ids(path, "BREWING_")
    # `ContextIntProviders.BREWING_DEFAULT_USES` is `brewing/uses_default`.
    names = {"default_uses": "uses_default"}
    uses = {item: read_json(jar, provider.format("int", f"brewing/{names[n]}")) for item, n in brewing_fuels.items()}
    mixes = brewing_recipes(jar)
    reagents = sorted({k[2] for k in mixes})

    def brewing_run(ops):
        b = models.BrewingStand({k: list(v) for k, v in mixes.items()}, set(reagents), uses, remainders)
        states = []
        for op in ops:
            if op[0] == "bottles":
                b.bottles = [tuple(x) if x else None for x in op[1]]
            elif op[0] == "ingredient":
                b.ingredient = (op[1], op[2])
            elif op[0] == "fuel":
                b.fuel_slot = (op[1], op[2])
            else:
                for _ in range(op[1]):
                    b.tick()
                states.append(b.state())
        return states

    # Enchanting.
    known, defs = enchantment_definitions(jar)
    source = expand_enchantments(jar, "in_enchanting_table", known)
    curses = expand_enchantments(jar, "curse", known)
    items = vector_items(server, inspect, items_class)
    repair = extract_java_repair_fixtures.extract(server)
    repair_sets = {}
    for r in repair["repairs"]:
        name = strip(r["item"])
        members = repair["tags"][r["tag"]] if "tag" in r else [r["material"]]
        repair_sets[name] = {m.split(":")[-1] for m in members}
    for name in items:
        items[name]["repairs"] = sorted(repair_sets.get(name, ()))
    offers = []
    for seed in [0, 1, -1, 42, 20261123, -987654321, 2147483647, -2147483648]:
        for shelves in [0, 1, 8, 15, 32]:
            for item in VECTOR_ITEMS:
                o = models.table_offers(seed, item, items[item]["enchantable"], shelves, source, defs)
                offers.append(dict({"seed": seed, "bookshelves": shelves, "item": item}, **o))
    anvil = []
    for name, left, right, rename, creative in anvil_cases():
        result = models.anvil_result(left, right, rename, creative, items, defs)
        anvil.append(
            {"case": name, "left": left, "right": right, "name": rename, "creative": creative, "result": result}
        )
    grindstone = []
    for name, a, b in grindstone_cases():
        grindstone.append(
            {
                "case": name,
                "first": a,
                "second": b,
                "result": models.grindstone_result(a, b, items, set(curses)),
                "xp": models.grindstone_xp(a, b, defs, set(curses)),
            }
        )

    # Smithing slots: every template, base and addition a recipe names.
    slots = {"template": set(), "base": set(), "addition": set()}
    prefix = "data/minecraft/recipe/"
    for n in sorted(jar.namelist()):
        if n.startswith(prefix) and n.endswith(".json"):
            r = read_json(jar, n)
            if r.get("type") in ("minecraft:smithing_transform", "minecraft:smithing_trim"):
                for slot in slots:
                    slots[slot] |= set(holder_items(jar, r[slot]))

    return {
        "furnace": {
            "constants": {
                "BURN_TIME_STANDARD": static(furnace_class, "BURN_TIME_STANDARD"),
                "BURN_COOL_SPEED": static(furnace_class, "BURN_COOL_SPEED"),
                "DEFAULT_SPEED_MULTIPLIER": static(furnace_class, "DEFAULT_SPEED_MULTIPLIER"),
            },
            "fast_cooking_blocks": fast_blocks,
            "speed_multipliers": speeds,
            "burn_time_reductions": reductions,
            "recipe_cooking_times": [list(t) for t in recipe_times],
            "fuels": fuels,
            "recipe_experience": dict(sorted(recipe_xp.items())),
            "scenarios": [
                {"case": n, "block": b, "ops": ops, "states": furnace_run(b, ops)} for n, b, ops in furnace_scenarios()
            ],
            "experience": [
                models.create_experience(c, v) for c in (1, 2, 3, 7, 10, 64) for v in (0.1, 0.15, 0.35, 0.7, 1.0, 2.0)
            ],
        },
        "brewing": {
            "constants": {
                "BREWING_TIME_SECONDS": static(brewing_class, "BREWING_TIME_SECONDS"),
                "DEFAULT_FUEL_USES": static(brewing_class, "DEFAULT_FUEL_USES"),
                "DEFAULT_SPEED_MULTIPLIER": static(brewing_class, "DEFAULT_SPEED_MULTIPLIER"),
                "speed_default": read_json(jar, provider.format("float", "brewing/speed_default")),
            },
            "fuels": uses,
            "potion_inputs": tag_values(jar, "data/minecraft/tags/item/brewing_potion_inputs.json"),
            "reagents": reagents,
            "scenarios": [{"case": n, "ops": ops, "states": brewing_run(ops)} for n, ops in brewing_scenarios()],
        },
        "comparator": {
            kind: [{"slots": c, "signal": models.comparator_signal(c, stacks)} for c in cases]
            for kind, cases in comparator_cases().items()
        },
        "enchanting": {
            "bookshelf_offsets": models.bookshelf_offsets(),
            "power_providers": sorted(expand_block_tag(jar, "enchantment_power_provider")),
            "power_transmitters": sorted(expand_block_tag(jar, "enchantment_power_transmitter")),
            "in_enchanting_table": source,
            "items": {name: items[name]["enchantable"] for name in VECTOR_ITEMS},
            "offers": offers,
        },
        "anvil": {
            "constants": {
                "MAX_NAME_LENGTH": static(anvil_menu, "MAX_NAME_LENGTH"),
                "too_expensive": 40,
                "rename_cap": 39,
                "wear_chance": 0.12,
                "falling_damage_per_block": 2.0,
                "falling_damage_max": 40,
            },
            "items": {n: {k: items[n][k] for k in ("max_damage", "repairs", "max_stack")} for n in ANVIL_ITEMS},
            "cases": anvil,
            "falling": [models.falling_anvil(d) for d in (0.5, 1.0, 1.5, 2.0, 3.0, 5.25, 10.0, 20.0, 21.0, 30.0)],
            "anvils": sorted(expand_block_tag(jar, "anvil")),
        },
        "grindstone": {"curses": curses, "cases": grindstone},
        "smithing": {slot: sorted(v) for slot, v in slots.items()},
    }


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
            facts.update(core_workstations(server, path, inspect, items_class, jar))
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
