#!/usr/bin/env python3
"""Extract Java 26.3's recipe book facts from verified official jars.

From the server jar: every learnable recipe (`Recipe.isSpecial` false) with
its recipe book category, group and `show_notification`, and the unlock
criteria of every recipe advancement under
`data/minecraft/advancement/recipes/` (items, item tags, occupied slots,
`tick`, `enter_block`), with the item tags they name expanded.

From the client jar: the tabs of each book (`CraftingRecipeBookComponent`,
`FurnaceScreen`, `BlastFurnaceScreen`, `SmokerScreen`), the search tabs'
categories (`SearchRecipeBookCategory`), the category registry order
(`RecipeBookCategories`), the per-book settings keys
(`RecipeBookSettings$TypeSettings`), layout and timing constants (static
final fields of the recipe book widgets and `RecipeToast`) and the English
labels from `assets/minecraft/lang/en_us.json`.

Class files are read for constants and for the order of static field and
string references in their static initializers; no Java code is copied.
The output is `tests/fixtures/java/26.3/recipe_book.json`.
"""

import argparse
import hashlib
import io
import json
import struct
import zipfile
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1
from extract_java_registry_names import CLIENT_SHA1

SERVER_SOURCE = f"https://piston-data.mojang.com/v1/objects/{SERVER_SHA1}/server.jar"
CLIENT_SOURCE = f"https://piston-data.mojang.com/v1/objects/{CLIENT_SHA1}/client.jar"
RECIPES = "data/minecraft/recipe/"
ADVANCEMENTS = "data/minecraft/advancement/recipes/"
TAGS = "data/minecraft/tags/item/"
PKG = "net/minecraft/"

# `Recipe.recipeBookCategory()` per recipe class (`CraftingRecipe`,
# `SmeltingRecipe`, `BlastingRecipe`, `SmokingRecipe`,
# `CampfireCookingRecipe`, `StonecutterRecipe`, `SmithingRecipe`), keyed by
# the JSON type and its `category` field. A missing `category` reads as
# `misc` (`CraftingBookInfo` and `CookingBookInfo` map codecs).
CATEGORY = {
    "crafting": {
        "building": "crafting_building_blocks",
        "equipment": "crafting_equipment",
        "redstone": "crafting_redstone",
        "misc": "crafting_misc",
    },
    "smelting": {"blocks": "furnace_blocks", "food": "furnace_food", "misc": "furnace_misc"},
    "blasting": {"blocks": "blast_furnace_blocks", "food": "blast_furnace_misc", "misc": "blast_furnace_misc"},
    "smoking": {"blocks": "smoker_food", "food": "smoker_food", "misc": "smoker_food"},
    "campfire_cooking": {"blocks": "campfire", "food": "campfire", "misc": "campfire"},
}
CRAFTING_TYPES = {"crafting_shaped", "crafting_shapeless", "crafting_transmute", "crafting_dye", "crafting_imbue"}
# `CustomRecipe` subclasses and `BrewingRecipe` are special: never learnable.
SPECIAL_TYPES = {"crafting_decorated_pot"}
BOOKS = {
    "crafting": "client/gui/screens/recipebook/CraftingRecipeBookComponent",
    "furnace": "client/gui/screens/inventory/FurnaceScreen",
    "blast_furnace": "client/gui/screens/inventory/BlastFurnaceScreen",
    "smoker": "client/gui/screens/inventory/SmokerScreen",
}
CONSTANTS = {
    "client/gui/screens/recipebook/RecipeBookComponent": [
        "IMAGE_WIDTH",
        "IMAGE_HEIGHT",
        "OFFSET_X_POSITION",
        "BORDER_WIDTH",
        "TICKS_TO_SWAP_SLOT",
    ],
    "client/gui/screens/recipebook/RecipeBookPage": [
        "ITEMS_PER_PAGE",
        "TURN_PAGE_SPRITE_WIDTH",
        "TURN_PAGE_SPRITE_HEIGHT",
    ],
    "client/gui/screens/recipebook/RecipeButton": ["ANIMATION_TIME", "BACKGROUND_SIZE"],
    "client/gui/components/toasts/RecipeToast": ["DISPLAY_TIME"],
}
LABELS = [
    "gui.recipebook.toggleRecipes.all",
    "gui.recipebook.toggleRecipes.craftable",
    "gui.recipebook.toggleRecipes.smeltable",
    "gui.recipebook.toggleRecipes.blastable",
    "gui.recipebook.toggleRecipes.smokable",
    "gui.recipebook.search_hint",
    "gui.recipebook.page",
    "gui.recipebook.next_page",
    "gui.recipebook.previous_page",
    "gui.recipebook.moreRecipes",
    "recipe.toast.title",
    "recipe.toast.description",
]


def short(name):
    if not name.startswith(("minecraft:", "#minecraft:")):
        raise ValueError(f"non-vanilla id {name}")
    return name.replace("minecraft:", "")


# ---------------------------------------------------------------- class files


class ClassFile:
    """The constant pool, static constants and static initializer of one class."""

    def __init__(self, data):
        if data[:4] != b"\xca\xfe\xba\xbe":
            raise ValueError("not a class file")
        self.data = data
        self.pos = 10
        count = self.u2_at(8)
        self.pool = [None] * count
        i = 1
        while i < count:
            tag = self.u1()
            if tag == 1:
                n = self.u2()
                self.pool[i] = ("utf8", self.data[self.pos : self.pos + n].decode("utf-8", "replace"))
                self.pos += n
            elif tag in (3, 4):
                raw = self.data[self.pos : self.pos + 4]
                self.pos += 4
                self.pool[i] = (
                    ("int", struct.unpack(">i", raw)[0]) if tag == 3 else ("float", struct.unpack(">f", raw)[0])
                )
            elif tag in (5, 6):
                raw = self.data[self.pos : self.pos + 8]
                self.pos += 8
                self.pool[i] = (
                    ("long", struct.unpack(">q", raw)[0]) if tag == 5 else ("double", struct.unpack(">d", raw)[0])
                )
                i += 1
            elif tag in (7, 8, 16, 19, 20):
                self.pool[i] = ({7: "class", 8: "string"}.get(tag, "other"), self.u2())
            elif tag in (9, 10, 11, 12, 17, 18):
                self.pool[i] = (
                    {9: "field", 10: "method", 11: "imethod", 12: "nat"}.get(tag, "other"),
                    self.u2(),
                    self.u2(),
                )
            elif tag == 15:
                self.pool[i] = ("handle", self.u1(), self.u2())
            else:
                raise ValueError(f"constant tag {tag}")
            i += 1
        self.pos += 6  # access, this, super
        interfaces = self.u2()
        self.pos += 2 * interfaces
        self.fields = {}
        for _ in range(self.u2()):
            self.pos += 2
            name = self.utf8(self.u2())
            self.pos += 2
            for attr, body in self.attributes():
                if attr == "ConstantValue":
                    self.fields[name] = self.constant(struct.unpack(">H", body)[0])
        self.clinit = None
        for _ in range(self.u2()):
            self.pos += 2
            name = self.utf8(self.u2())
            self.pos += 2
            for attr, body in self.attributes():
                if attr == "Code" and name == "<clinit>":
                    length = struct.unpack(">I", body[4:8])[0]
                    self.clinit = body[8 : 8 + length]

    def u1(self):
        self.pos += 1
        return self.data[self.pos - 1]

    def u2(self):
        self.pos += 2
        return self.u2_at(self.pos - 2)

    def u2_at(self, at):
        return struct.unpack(">H", self.data[at : at + 2])[0]

    def attributes(self):
        out = []
        for _ in range(self.u2()):
            name = self.utf8(self.u2())
            length = struct.unpack(">I", self.data[self.pos : self.pos + 4])[0]
            self.pos += 4
            out.append((name, self.data[self.pos : self.pos + length]))
            self.pos += length
        return out

    def utf8(self, index):
        kind, value = self.pool[index]
        if kind != "utf8":
            raise ValueError("expected utf8")
        return value

    def constant(self, index):
        entry = self.pool[index]
        if entry[0] == "string":
            return self.utf8(entry[1])
        return entry[1]

    def member(self, index):
        """(owner class, name) of a field or method reference."""
        _, owner, nat = self.pool[index]
        return self.utf8(self.pool[owner][1]), self.utf8(self.pool[nat][1])

    def static_events(self):
        """The static initializer's `ldc` strings and `getstatic`/`putstatic`
        field references, in order."""
        code, i, out = self.clinit or b"", 0, []
        while i < len(code):
            op = code[i]
            if op == 0x12:  # ldc
                out.append(("ldc", self.ldc(code[i + 1])))
            elif op in (0x13, 0x14):  # ldc_w, ldc2_w
                out.append(("ldc", self.ldc(struct.unpack(">H", code[i + 1 : i + 3])[0])))
            elif op in (0xB2, 0xB3):  # getstatic, putstatic
                ref = struct.unpack(">H", code[i + 1 : i + 3])[0]
                out.append(("get" if op == 0xB2 else "put", self.member(ref)))
            i += opcode_length(code, i)
        return out

    def ldc(self, index):
        entry = self.pool[index]
        return self.utf8(entry[1]) if entry[0] == "string" else entry[1] if entry[0] != "class" else None


def opcode_length(code, i):
    op = code[i]
    if op == 0xAA:  # tableswitch
        p = (i + 4) & ~3
        low, high = struct.unpack(">ii", code[p + 4 : p + 12])
        return p + 12 + 4 * (high - low + 1) - i
    if op == 0xAB:  # lookupswitch
        p = (i + 4) & ~3
        pairs = struct.unpack(">i", code[p + 4 : p + 8])[0]
        return p + 8 + 8 * pairs - i
    if op == 0xC4:  # wide
        return 6 if code[i + 1] == 0x84 else 4
    two = {0x10, 0x12, 0x15, 0x16, 0x17, 0x18, 0x19, 0x36, 0x37, 0x38, 0x39, 0x3A, 0xA9, 0xBC}
    three = {0x11, 0x13, 0x14, 0x84, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xBB, 0xBD, 0xC0, 0xC1, 0xC6, 0xC7}
    three |= set(range(0x99, 0xA9))
    if op in two:
        return 2
    if op in three:
        return 3
    if op in (0xB9, 0xBA, 0xC8, 0xC9):
        return 5
    if op == 0xC5:
        return 4
    return 1


# ---------------------------------------------------------------- client facts


def client_facts(client, items):
    def cls(name):
        return ClassFile(client.read(f"{PKG}{name}.class"))

    items_class = f"{PKG}world/item/Items"
    categories_class = f"{PKG}world/item/crafting/RecipeBookCategories"
    search_class = f"{PKG}client/gui/screens/recipebook/SearchRecipeBookCategory"

    registry = [e[1] for e in cls("world/item/crafting/RecipeBookCategories").static_events() if e[0] == "ldc"]

    def category(field):
        name = field.lower()
        if name not in registry:
            raise ValueError(f"unknown recipe book category {field}")
        return name

    search, current = {}, []
    for kind, value in cls("client/gui/screens/recipebook/SearchRecipeBookCategory").static_events():
        if kind == "get" and value[0] == categories_class:
            current.append(category(value[1]))
        elif kind == "put" and value[0] == search_class and value[1] != "$VALUES":
            search[value[1].lower()] = current
            current = []

    tabs, filters = {}, {}
    for book, name in BOOKS.items():
        out, icons = [], []
        for kind, value in cls(name).static_events():
            if kind == "ldc" and isinstance(value, str) and value.startswith("gui.recipebook.toggleRecipes."):
                filters[book] = value
            if kind != "get":
                continue
            owner, field = value
            if owner == items_class:
                item = field.lower()
                if item not in items:
                    raise ValueError(f"unknown tab icon {field}")
                icons.append(item)
            elif owner == search_class:
                out.append({"search": field.lower(), "icons": ["compass"]})
            elif owner == categories_class:
                out.append({"category": category(field), "icons": icons})
                icons = []
        if icons or not out or "search" not in out[0]:
            raise ValueError(f"unexpected tab layout in {name}")
        tabs[book] = out
    # `CraftingRecipeBookComponent` names its filter in a field initializer.
    filters.setdefault("crafting", "gui.recipebook.toggleRecipes.craftable")

    settings = {}
    fields = ["crafting", "furnace", "blast_furnace", "smoker"]
    keys = [
        e[1]
        for e in cls("stats/RecipeBookSettings$TypeSettings").static_events()
        if e[0] == "ldc" and isinstance(e[1], str)
    ]
    if len(keys) != 8:
        raise ValueError(f"unexpected settings keys {keys}")
    for i, book in enumerate(fields):
        settings[book] = {"open": keys[2 * i], "filtering": keys[2 * i + 1]}

    constants = {}
    for name, wanted in CONSTANTS.items():
        found = cls(name).fields
        constants[name.rsplit("/", 1)[1]] = {k: found[k] for k in wanted}

    lang = json.loads(client.read("assets/minecraft/lang/en_us.json"))
    labels = {key: lang[key] for key in LABELS}
    return {
        "categories": registry,
        "search_tabs": search,
        "tabs": tabs,
        "filter_labels": filters,
        "settings_keys": settings,
        "constants": constants,
        "labels": labels,
    }


# ---------------------------------------------------------------- server facts


def server_facts(jar):
    recipes, special = {}, []
    for path in sorted(n for n in jar.namelist() if n.startswith(RECIPES) and n.endswith(".json")):
        rid = path[len(RECIPES) : -5]
        if "/" in rid:  # brewing/: `BrewingRecipe.isSpecial`
            continue
        recipe = json.loads(jar.read(path))
        kind = short(recipe["type"])
        if kind.startswith("crafting_special_") or kind in SPECIAL_TYPES:
            special.append(rid)
            continue
        if kind in CRAFTING_TYPES:
            book = CATEGORY["crafting"][recipe.get("category", "misc")]
        elif kind in CATEGORY:
            book = CATEGORY[kind][recipe.get("category", "misc")]
        elif kind == "stonecutting":
            book = "stonecutter"
        elif kind.startswith("smithing_"):
            book = "smithing"
        else:
            raise ValueError(f"{rid}: unknown recipe type {kind}")
        fact = {"type": kind, "category": book}
        if recipe.get("group"):
            fact["group"] = recipe["group"]
        if recipe.get("show_notification", True) is not True:
            fact["notify"] = False
        recipes[rid] = fact

    unlocks, tags = {}, set()
    for path in sorted(n for n in jar.namelist() if n.startswith(ADVANCEMENTS) and n.endswith(".json")):
        name = path[len(ADVANCEMENTS) : -5]
        if name == "root":
            continue
        adv = json.loads(jar.read(path))
        criteria = adv["criteria"]
        if adv.get("requirements") != [sorted(criteria)] and sorted(adv["requirements"][0]) != sorted(criteria):
            raise ValueError(f"{name}: requirements are not one group of every criterion")
        if len(adv["requirements"]) != 1:
            raise ValueError(f"{name}: more than one requirement group")
        rewards = [short(r) for r in adv["rewards"]["recipes"]]
        fact = {"advancement": name}
        items = []
        for key, criterion in sorted(criteria.items()):
            trigger = short(criterion["trigger"])
            cond = criterion.get("conditions", {})
            if trigger == "recipe_unlocked":
                if [short(cond["recipes"])] != rewards:
                    raise ValueError(f"{name}: {key} names another recipe")
            elif trigger == "inventory_changed" and set(cond) == {"items"}:
                (predicate,) = cond["items"]
                if set(predicate) != {"items"} or not isinstance(predicate["items"], str):
                    raise ValueError(f"{name}: unsupported item predicate {predicate}")
                item = short(predicate["items"])
                items.append(item)
                if item.startswith("#"):
                    tags.add(item[1:])
            elif trigger == "inventory_changed" and set(cond) == {"slots"}:
                if cond["slots"] != {"occupied": {"min": cond["slots"]["occupied"]["min"]}}:
                    raise ValueError(f"{name}: unsupported slots predicate")
                fact["occupied_min"] = cond["slots"]["occupied"]["min"]
            elif trigger == "tick" and not cond:
                fact["tick"] = True
            elif trigger == "enter_block" and set(cond) == {"blocks"}:
                fact.setdefault("enter_blocks", []).append(short(cond["blocks"]))
            else:
                raise ValueError(f"{name}: unsupported criterion {key} {criterion}")
        if items:
            fact["items"] = sorted(set(items))
        for rid in rewards:
            if rid not in recipes:
                raise ValueError(f"{name}: rewards unknown or special recipe {rid}")
            if rid in unlocks:
                raise ValueError(f"{rid}: rewarded twice")
            unlocks[rid] = fact

    cache = {}

    def expand(tag, visiting=()):
        if tag in cache:
            return cache[tag]
        if tag in visiting:
            raise ValueError(f"tag cycle at {tag}")
        out = []
        for value in json.loads(jar.read(f"{TAGS}{tag}.json"))["values"]:
            if isinstance(value, dict):
                if value.get("required", True):
                    raise ValueError(f"required object entry in #{tag}")
                value = value["id"]
            value = short(value)
            for item in expand(value[1:], (*visiting, tag)) if value.startswith("#") else [value]:
                if item not in out:
                    out.append(item)
        cache[tag] = out
        return out

    item_tags = {tag: expand(tag) for tag in sorted(tags)}
    without = sorted(r for r in recipes if r not in unlocks)
    return recipes, sorted(special), unlocks, item_tags, without


def verified(path, sha1):
    data = path.read_bytes()
    if hashlib.sha1(data).hexdigest() != sha1:
        raise ValueError(f"{path} is not the selected official Java 26.3 artifact")
    return zipfile.ZipFile(io.BytesIO(data))


def extract(server_path, client_path):
    bundler = verified(server_path, SERVER_SHA1)
    jar = zipfile.ZipFile(io.BytesIO(bundler.read("META-INF/versions/26.3/server-26.3.jar")))
    client = verified(client_path, CLIENT_SHA1)
    items = {
        n[len("assets/minecraft/items/") : -5]
        for n in client.namelist()
        if n.startswith("assets/minecraft/items/") and n.endswith(".json")
    }
    recipes, special, unlocks, item_tags, without = server_facts(jar)
    facts = client_facts(client, items)
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_source": SERVER_SOURCE,
        "server_sha1": SERVER_SHA1,
        "client_source": CLIENT_SOURCE,
        "client_sha1": CLIENT_SHA1,
        "sources": [
            f"{RECIPES}*.json (type, category, group, show_notification)",
            f"{ADVANCEMENTS}**/*.json (criteria and rewards)",
            f"{TAGS}*.json (tags the criteria name, expanded)",
            "client classes: recipe book components, furnace screens, SearchRecipeBookCategory, "
            "RecipeBookCategories, RecipeBookSettings$TypeSettings, RecipeToast",
            "assets/minecraft/lang/en_us.json",
        ],
        **facts,
        "counts": {
            "learnable": len(recipes),
            "special": len(special),
            "unlocks": len(unlocks),
            "without_unlock": len(without),
        },
        "special": special,
        "without_unlock": without,
        "item_tags": item_tags,
        "learnable": recipes,
        "unlocks": dict(sorted(unlocks.items())),
    }


def dump(fixture):
    """Small facts on their own lines; one recipe, unlock or tag per line."""

    def compact(value):
        return json.dumps(value, separators=(",", ":"))

    lines = ["{"]
    top = list(fixture.items())
    for i, (key, value) in enumerate(top):
        end = "," if i + 1 < len(top) else ""
        if isinstance(value, dict) and key in ("item_tags", "learnable", "unlocks", "tabs", "constants", "labels"):
            lines.append(f"  {compact(key)}: {{")
            rows = list(value.items())
            for j, (name, entry) in enumerate(rows):
                lines.append(f"    {compact(name)}: {compact(entry)}{',' if j + 1 < len(rows) else ''}")
            lines.append(f"  }}{end}")
        else:
            lines.append(f"  {compact(key)}: {compact(value)}{end}")
    lines.append("}")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="the official 26.3 server.jar (bundler)")
    parser.add_argument("client", type=Path, help="the official 26.3 client.jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        fixture = extract(args.server, args.client)
    except (OSError, ValueError, KeyError, zipfile.BadZipFile) as error:
        parser.error(str(error))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(dump(fixture), encoding="utf-8")


if __name__ == "__main__":
    main()
