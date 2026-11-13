#!/usr/bin/env python3
"""Extract the bundle facts from the pinned official Java 26.3 server artifact.

Covers the `bundles` item tag (plain bundle, then `DyeColor` order), the
`Items.BUNDLE` and `Items.DYED_BUNDLE` registrations (`stacksTo(1)` with an
empty `BUNDLE_CONTENTS`), `BundleContents`' weight constants
(`BUNDLE_IN_BUNDLE_WEIGHT`, `BEEHIVE_WEIGHT`), the maximum stack sizes of the
items the cases use (from `Items`' static initializer and
`Item.DEFAULT_MAX_STACK_SIZE`), `bundle.json` and the sixteen
`<colour>_bundle.json` transmute recipes, and every loot table pool that
names a bundle.

It then runs a small independent model of `BundleContents.Mutable`
(`tryInsert`, `tryTransfer`, `removeOne`, `getMaxAmountToAdd`) and of
`BundleItem.overrideStackedOnOther` / `overrideOtherStackedOnMe`, written
from the decompiled 26.3 classes with exact fractions, over a table of
insert, remove and click cases. The game's results are compared with that
table in `tests/java_bundles.rs`.

Requires javap. Unknown bytecode forms fail rather than regenerating a guessed
fixture. No VoxelCraft source or existing fixture is consulted.
"""
import argparse
import copy
from fractions import Fraction
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1

# The items the cases use, by Java id.
CASE_ITEMS = ["stone", "leather", "snowball", "ender_pearl", "bucket", "mushroom_stew", "saddle", "beehive", "bundle"]


def strip(name):
    if not name.startswith("minecraft:"):
        raise ValueError(f"not a minecraft id: {name}")
    return name[len("minecraft:"):]


def stack_size(items, default, field):
    """`stacksTo(n)` in the static-initializer window that registers `field`."""
    upper = field.upper()
    start = re.search(r"getstatic[^\n]*Field net/minecraft/references/(Block)?ItemIds\." + upper + r":", items)
    if not start:
        raise ValueError(f"no registration for {field}")
    end = re.search(r"putstatic[^\n]*Field " + upper + r":", items[start.end():])
    if not end:
        raise ValueError(f"no putstatic for {field}")
    window = items[start.start():start.end() + end.end()]
    if re.search(r"Properties\.(durability|sword|pickaxe|axe|shovel|hoe|humanoidArmor)", window):
        raise ValueError(f"{field} has durability; this extractor only reads explicit stack sizes")
    if "invokedynamic" in window and re.search(r"lambda\$", window):
        raise ValueError(f"{field} registers through a lambda; read it by hand")
    sizes = re.findall(r"(iconst_(\d)|bipush\s+(\d+))\n[^\n]*Item\$Properties\.stacksTo", window)
    if len(sizes) > 1:
        raise ValueError(f"{field} calls stacksTo twice")
    if sizes:
        return int(sizes[0][1] or sizes[0][2])
    if "stacksTo" in window:
        raise ValueError(f"unrecognized stacksTo for {field}")
    return default


# --- An independent model of BundleContents.Mutable and BundleItem ----------

class Model:
    def __init__(self, max_stack):
        self.max_stack = max_stack

    def stack(self, item, count, name=None, contents=None, bees=False):
        s = {"item": item, "count": count}
        if name is not None:
            s["name"] = name
        if contents is not None:
            s["contents"] = contents
        if bees:
            s["bees"] = True
        return s

    def is_bundle(self, s):
        return s["item"].endswith("bundle")

    def weight(self, s):
        """`getWeight`: one item's weight."""
        if self.is_bundle(s):
            return self.contents_weight(s.get("contents", [])) + Fraction(1, 16)
        if s.get("bees"):
            return Fraction(1)
        return Fraction(1, self.max_stack[s["item"]])

    def contents_weight(self, items):
        return sum((self.weight(s) * s["count"] for s in items), Fraction(0))

    def same(self, a, b):
        """`isSameItemSameComponents`."""
        strip_count = lambda s: {k: v for k, v in s.items() if k != "count"}
        return strip_count(a) == strip_count(b)

    def max_amount(self, contents, w):
        """`getMaxAmountToAdd`: `Math.max(remaining.divideBy(w).intValue(), 0)`."""
        remaining = 1 - self.contents_weight(contents)
        return max(int(remaining / w), 0)

    def try_insert(self, contents, offered):
        """`tryInsert`: returns (amount, what is left of `offered`)."""
        if offered is None:
            return 0, None
        amount = min(offered["count"], self.max_amount(contents, self.weight(offered)))
        if amount == 0:
            return 0, offered
        stackable = self.max_stack[offered["item"]] > 1
        index = next((i for i, s in enumerate(contents) if stackable and self.same(s, offered)), -1)
        if index >= 0:
            merged = contents.pop(index)
            merged = dict(merged, count=merged["count"] + amount)
        else:
            merged = dict(offered, count=amount)
        contents.insert(0, merged)
        left = offered["count"] - amount
        return amount, (dict(offered, count=left) if left > 0 else None)

    def remove_one(self, contents):
        return contents.pop(0) if contents else None

    def set_contents(self, bundle, contents):
        out = dict(bundle)
        if contents:
            out["contents"] = contents
        else:
            out.pop("contents", None)
        return out

    def click(self, carried, slot, right):
        """`tryItemClickBehaviourOverride`: carried first, then clicked."""
        if carried is not None and self.is_bundle(carried):
            contents = copy.deepcopy(carried.get("contents", []))
            if not right and slot is not None:
                # `tryTransfer`: `safeTake(count, maxAmount)` then `tryInsert`.
                take = min(slot["count"], self.max_amount(contents, self.weight(slot)))
                taken = dict(slot, count=take) if take > 0 else None
                moved, _ = self.try_insert(contents, taken)
                left = slot["count"] - moved
                slot = dict(slot, count=left) if left > 0 else None
                return self.set_contents(carried, contents), slot, "inserted" if moved > 0 else "insert_failed"
            if right and slot is None:
                out = self.remove_one(contents)
                outcome = "nothing"
                if out is not None:
                    fits = min(out["count"], self.max_stack[out["item"]])
                    slot = dict(out, count=fits)
                    if out["count"] > fits:
                        self.try_insert(contents, dict(out, count=out["count"] - fits))
                    else:
                        outcome = "removed"
                return self.set_contents(carried, contents), slot, outcome
        if slot is not None and self.is_bundle(slot):
            contents = copy.deepcopy(slot.get("contents", []))
            if not right and carried is not None:
                moved, carried = self.try_insert(contents, carried)
                return carried, self.set_contents(slot, contents), "inserted" if moved > 0 else "insert_failed"
            if right and carried is None:
                out = self.remove_one(contents)
                return out, self.set_contents(slot, contents), "removed" if out is not None else "nothing"
        return None


def fraction(f):
    return f"{f.numerator}/{f.denominator}"


def cases(model):
    S = model.stack
    inner16 = S("bundle", 1, contents=[S("stone", 16)])
    inner32 = S("bundle", 1, contents=[S("stone", 32)])
    deep = S("red_bundle", 1, contents=[S("bundle", 1, contents=[S("bundle", 1, contents=[S("snowball", 4)])])])
    sequences = [
        ("a full stack of a 64-stacking item fills a bundle", [], [("insert", S("stone", 64)), ("insert", S("stone", 1))]),
        ("partial stacks merge up to the weight limit", [], [("insert", S("stone", 40)), ("insert", S("stone", 40))]),
        ("sixteen of a 16-stacking item fill a bundle", [], [("insert", S("ender_pearl", 16)), ("insert", S("leather", 1))]),
        ("eight pearls and thirty-two stone fill a bundle exactly", [],
         [("insert", S("ender_pearl", 8)), ("insert", S("stone", 32)), ("insert", S("stone", 1))]),
        ("the remaining weight is floored", [S("stone", 33)], [("insert", S("ender_pearl", 16)), ("insert", S("bucket", 16))]),
        ("an unstackable item weighs one", [], [("insert", S("mushroom_stew", 1)), ("insert", S("stone", 1))]),
        ("an unstackable item needs an empty bundle", [S("stone", 1)], [("insert", S("saddle", 1))]),
        ("merging moves the stack to the front", [],
         [("insert", S("stone", 10)), ("insert", S("leather", 5)), ("insert", S("stone", 3)),
          ("insert", S("stone", 2, name="A")), ("remove",), ("remove",), ("remove",), ("remove",)]),
        ("removing takes the most recent stack first", [],
         [("insert", S("stone", 1)), ("insert", S("leather", 1)), ("insert", S("snowball", 1)),
          ("remove",), ("remove",), ("remove",), ("remove",)]),
        ("empty bundles are unstackable and weigh a sixteenth each", [],
         [("insert", S("bundle", 1)), ("insert", S("white_bundle", 1)), ("insert", S("bundle", 1)), ("insert", S("stone", 64))]),
        ("a nested bundle weighs a sixteenth plus its contents", [S("stone", 32)],
         [("insert", inner32), ("insert", inner16), ("insert", S("stone", 64))]),
        ("bundles nest three deep", [], [("insert", deep), ("insert", S("ender_pearl", 16))]),
        ("a hive holding bees weighs one", [], [("insert", S("beehive", 1, bees=True)), ("insert", S("stone", 1))]),
        ("a hive holding bees needs an empty bundle", [S("stone", 1)], [("insert", S("beehive", 1, bees=True))]),
        ("an empty hive weighs one sixty-fourth", [S("stone", 54)], [("insert", S("beehive", 16)), ("insert", S("beehive", 1, bees=True))]),
    ]
    out_sequences = []
    for name, start, ops in sequences:
        contents = copy.deepcopy(start)
        results = []
        for op in ops:
            if op[0] == "insert":
                amount, left = model.try_insert(contents, copy.deepcopy(op[1]))
                results.append({"insert": op[1], "accepted": amount})
            else:
                results.append({"remove": True, "removed": model.remove_one(contents)})
            results[-1]["contents"] = copy.deepcopy(contents)
            results[-1]["weight"] = fraction(model.contents_weight(contents))
        out_sequences.append({"name": name, "start": start, "steps": results})

    full = [S("stone", 64)]
    clicks = [
        ("held bundle takes a slot's stack", S("bundle", 1), S("stone", 64), False),
        ("held bundle takes what fits", S("bundle", 1, contents=[S("stone", 32)]), S("ender_pearl", 16), False),
        ("held full bundle takes nothing", S("bundle", 1, contents=full), S("leather", 5), False),
        ("held bundle empties its front stack into an empty slot", S("blue_bundle", 1, contents=[S("leather", 3), S("stone", 2)]), None, True),
        ("held empty bundle right-clicks an empty slot", S("bundle", 1), None, True),
        ("held bundle left-clicks an empty slot", S("bundle", 1, contents=[S("stone", 2)]), None, False),
        ("held bundle right-clicks a stack", S("bundle", 1), S("stone", 3), True),
        ("a stack goes into a clicked bundle", S("stone", 64), S("bundle", 1), False),
        ("a clicked bundle takes all that fits", S("stone", 10), S("bundle", 1, contents=[S("ender_pearl", 12)]), False),
        ("a clicked bundle takes part of a stack", S("stone", 40), S("bundle", 1, contents=[S("ender_pearl", 12)]), False),
        ("a full clicked bundle refuses a stack", S("leather", 1), S("bundle", 1, contents=full), False),
        ("right-clicking a bundle with nothing held takes its front stack", None, S("bundle", 1, contents=[S("leather", 3), S("stone", 2)]), True),
        ("right-clicking an empty bundle with nothing held", None, S("bundle", 1), True),
        ("left-clicking a bundle with nothing held picks it up", None, S("bundle", 1, contents=[S("stone", 2)]), False),
        ("right-clicking a bundle with a stack held swaps", S("stone", 3), S("bundle", 1), True),
        ("a held bundle takes a clicked bundle", S("bundle", 1), S("green_bundle", 1, contents=[S("stone", 16)]), False),
        ("a held bundle takes a hive holding bees", S("bundle", 1), S("beehive", 1, bees=True), False),
        ("a held bundle with anything in it refuses a hive holding bees", S("bundle", 1, contents=[S("stone", 1)]), S("beehive", 1, bees=True), False),
        ("a held bundle and a clicked bundle on right click swap", S("bundle", 1), S("bundle", 1, contents=[S("stone", 1)]), True),
    ]
    out_clicks = []
    for name, carried, slot, right in clicks:
        result = model.click(copy.deepcopy(carried), copy.deepcopy(slot), right)
        case = {"name": name, "carried": carried, "slot": slot, "button": "right" if right else "left"}
        if result is None:
            case["overridden"] = False
        else:
            case.update({"overridden": True, "carried_after": result[0], "slot_after": result[1], "outcome": result[2]})
        out_clicks.append(case)
    return out_sequences, out_clicks


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-bundle-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        items = inspect("world.item.Items")
        item = inspect("world.item.Item")
        default = re.search(r"public static final int DEFAULT_MAX_STACK_SIZE = (\d+);", item)
        if not default:
            raise ValueError("missing Item.DEFAULT_MAX_STACK_SIZE")
        default = int(default[1])
        # BUNDLE: `stacksTo(1).component(BUNDLE_CONTENTS, EMPTY)`; DYED_BUNDLE:
        # a colour collection whose lambda registers the same properties.
        bundle_window = re.search(r"ItemIds\.BUNDLE:.*?putstatic[^\n]*Field BUNDLE:", items, re.S)
        if not bundle_window or not re.search(
                r"iconst_1\n[^\n]*stacksTo[^\n]*\n[^\n]*DataComponents\.BUNDLE_CONTENTS[^\n]*\n"
                r"[^\n]*BundleContents\.EMPTY", bundle_window[0]):
            raise ValueError("unrecognized Items.BUNDLE registration")
        if not re.search(r"ItemIds\.DYED_BUNDLE:[^\n]*\n[^\n]*invokedynamic[^\n]*\n[^\n]*registerItems[^\n]*\n"
                         r"[^\n]*putstatic[^\n]*Field DYED_BUNDLE:", items):
            raise ValueError("Items.DYED_BUNDLE is no longer one colour collection")
        lambdas = [m for m in items.split("\n\n")
                   if "BundleContents.EMPTY" in m and "private static net.minecraft.world.item.Item lambda$" in m]
        if len(lambdas) != 1 or not re.search(
                r"iconst_1\n[^\n]*stacksTo[^\n]*\n[^\n]*DataComponents\.BUNDLE_CONTENTS[^\n]*\n"
                r"[^\n]*BundleContents\.EMPTY", lambdas[0]):
            raise ValueError("unrecognized DYED_BUNDLE item lambda")
        max_stack = {name: stack_size(items, default, name) for name in CASE_ITEMS if name != "bundle"}
        max_stack["bundle"] = 1

        contents = inspect("world.item.component.BundleContents")
        if not re.search(r"iconst_1\n[^\n]*bipush\s+16\n[^\n]*Fraction\.getFraction:\(II\)[^\n]*\n"
                         r"[^\n]*putstatic[^\n]*Field BUNDLE_IN_BUNDLE_WEIGHT:", contents):
            raise ValueError("BUNDLE_IN_BUNDLE_WEIGHT is no longer getFraction(1, 16)")
        if not re.search(r"Fraction\.ONE:[^\n]*\n[^\n]*DataResult\.success[^\n]*\n"
                         r"[^\n]*putstatic[^\n]*Field BEEHIVE_WEIGHT:", contents):
            raise ValueError("BEEHIVE_WEIGHT is no longer success(Fraction.ONE)")
        mutable = inspect("world.item.component.BundleContents$Mutable")
        if not re.search(r"private int getMaxAmountToAdd\(.*?Fraction\.intValue.*?Math\.max", mutable, re.S):
            raise ValueError("getMaxAmountToAdd no longer truncates with intValue")

    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def read(name):
            return json.loads(jar.read(name))

        tag = [strip(v) for v in read("data/minecraft/tags/item/bundles.json")["values"]]
        if tag[0] != "bundle" or not all(n.endswith("_bundle") for n in tag[1:]):
            raise ValueError("unexpected bundles tag")
        colours = [n[:-len("_bundle")] for n in tag[1:]]
        plain = read("data/minecraft/recipe/bundle.json")
        recipes = {"bundle": {
            "type": strip(plain["type"]), "category": plain.get("category", ""),
            "pattern": plain["pattern"], "key": {k: strip(v) for k, v in plain["key"].items()},
            "result": strip(plain["result"]["id"]),
        }}
        for colour in colours:
            r = read(f"data/minecraft/recipe/{colour}_bundle.json")
            recipes[f"{colour}_bundle"] = {
                "type": strip(r["type"]), "category": r.get("category", ""), "group": r.get("group", ""),
                "input": r["input"], "material": strip(r["material"]), "result": strip(r["result"]["id"]),
                "material_count": r.get("material_count"), "add_material_count_to_result": r.get("add_material_count_to_result", False),
            }
        loot = {}
        for name in sorted(jar.namelist()):
            if not name.startswith("data/minecraft/loot_table/") or not name.endswith(".json"):
                continue
            table = read(name)
            for pool in table.get("pools", []):
                if any(e.get("name") in [f"minecraft:{t}" for t in tag] for e in pool["entries"]):
                    key = name[len("data/minecraft/loot_table/"):-len(".json")]
                    entries = []
                    for e in pool["entries"]:
                        functions = e.get("functions", e.get("modifier", []))
                        functions = [functions] if isinstance(functions, dict) else functions
                        counts = [f["count"] for f in functions
                                  if strip(f.get("function", f.get("type", ""))) == "set_count"]
                        if len(counts) != len(functions):
                            raise ValueError(f"{key}: unrecognized loot function")
                        entries.append({"type": strip(e["type"]), "name": strip(e["name"]) if "name" in e else None,
                                        "weight": e.get("weight", 1), "count": counts[0] if counts else 1})
                    loot.setdefault(key, []).append({"rolls": pool["rolls"], "entries": entries})

    # Every bundle stacks to one (both registrations checked above).
    model = Model(dict(max_stack, **{name: 1 for name in tag}))
    sequences, clicks = cases(model)
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.item.Items BUNDLE, DYED_BUNDLE (and the case items' stacksTo)",
            "net.minecraft.world.item.Item DEFAULT_MAX_STACK_SIZE",
            "net.minecraft.world.item.component.BundleContents (BUNDLE_IN_BUNDLE_WEIGHT, BEEHIVE_WEIGHT; Mutable modelled)",
            "net.minecraft.world.item.BundleItem overrideStackedOnOther, overrideOtherStackedOnMe (modelled)",
            "data/minecraft/tags/item/bundles.json",
            "data/minecraft/recipe/bundle.json, {colour}_bundle.json",
            "data/minecraft/loot_table/**/*.json (pools naming a bundle)",
        ],
        "items": tag,
        "colours": colours,
        "stack_size": 1,
        "weights": {"bundle_in_bundle": "1/16", "beehive": "1/1", "default_max_stack_size": default},
        "max_stack": max_stack,
        "recipes": recipes,
        "loot": loot,
        "sequences": sequences,
        "clicks": clicks,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
