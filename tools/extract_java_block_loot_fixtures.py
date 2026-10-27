#!/usr/bin/env python3
"""Extract every block loot table from the pinned official Java 26.3 artifact.

Reads `data/minecraft/loot_table/blocks/*.json`, the `tool/can_shear` and
`tool/can_silk_touch` predicates, the item tags the tables and
`DecoratedPotBlock.playerWillDestroy` use, and `Blocks` bytecode (javap) for the
blocks that have no table file of their own: wall variants share their standing
block's table (`Blocks.wallVariant` calls `overrideLootTable`), every other one
drops nothing. The bonus formulas, `table_bonus`, `set_count` and the uniform and
binomial providers are checked in bytecode before they are modelled.

Each table is evaluated exactly (probability distributions, no sampling) for
every combination of the block-state values its conditions name and for each
mining scenario below. A state value no condition names is written `*`. Mining
has no explosion, so `survives_explosion` passes and `explosion_decay` keeps
every item; `location_check` (the other half of a double plant) and
`entity_properties` on `this` (a player broke the block) pass. Unknown entry
types, conditions, functions or keys fail instead of producing a guessed
fixture. No VoxelCraft source or existing fixture is consulted.
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

ROOT = "data/minecraft/loot_table/blocks/"
COLORS = ("white", "orange", "magenta", "light_blue", "yellow", "lime", "pink", "gray", "light_gray", "cyan",
          "purple", "blue", "brown", "green", "red", "black")
# Tool flags the tables can observe: shears (`tool/can_shear`), Silk Touch
# (`tool/can_silk_touch`), the Fortune level (`apply_bonus`, `table_bonus`) and a
# pickaxe (`#cluster_max_harvestables`).
SCENARIOS = {
    "hand": {"shears": False, "silk_touch": False, "fortune": 0, "pickaxe": False},
    "pickaxe": {"shears": False, "silk_touch": False, "fortune": 0, "pickaxe": True},
    "shears": {"shears": True, "silk_touch": False, "fortune": 0, "pickaxe": False},
    "silk_touch": {"shears": False, "silk_touch": True, "fortune": 0, "pickaxe": True},
    "fortune_1": {"shears": False, "silk_touch": False, "fortune": 1, "pickaxe": True},
    "fortune_2": {"shears": False, "silk_touch": False, "fortune": 2, "pickaxe": True},
    "fortune_3": {"shears": False, "silk_touch": False, "fortune": 3, "pickaxe": True},
}
EPSILON = 1e-12


def method(code, signature):
    parts = code.split("  " + signature)
    if len(parts) < 2:
        raise ValueError(f"missing reference method: {signature}")
    return parts[1].split("\n\n")[0]


def as_list(value):
    return [] if value is None else value if isinstance(value, list) else [value]


def check_bytecode(inspect):
    """The formulas the evaluator models, checked against the reference."""
    bonus = "net.minecraft.world.level.storage.loot.functions.ApplyBonusCount$"
    calc = "public int calculateNewCount(net.minecraft.util.RandomSource, int, int);"
    ore = method(inspect(bonus + "OreDrops"), calc)
    if not re.search(r"iload_3\n.*ifle.*\n.*aload_1\n.*iload_3\n.*iconst_2\n.*iadd\n.*nextInt:\(I\)I\n.*iconst_1\n"
                     r".*isub\n.*istore\s+4\n.*iload\s+4\n.*ifge.*\n.*iconst_0\n.*istore\s+4\n.*iload_2\n.*iload\s+4\n"
                     r".*iconst_1\n.*iadd\n.*imul", ore):
        raise ValueError("ore_drops is no longer count * (max(0, nextInt(level + 2) - 1) + 1)")
    uniform = method(inspect(bonus + "UniformBonusCount"), calc)
    if not re.search(r"iload_2\n.*aload_1\n.*aload_0\n.*bonusMultiplier:I\n.*iload_3\n.*imul\n.*iconst_1\n.*iadd\n"
                     r".*nextInt:\(I\)I\n.*iadd", uniform):
        raise ValueError("uniform_bonus_count is no longer count + nextInt(multiplier * level + 1)")
    binomial = method(inspect(bonus + "BinomialWithBonusCount"), calc)
    if not re.search(r"iload_3\n.*aload_0\n.*extraRounds:I\n.*iadd\n.*if_icmpge(?:.*\n){1,3}.*nextFloat:\(\)F\n"
                     r".*aload_0\n.*probability:F\n.*fcmpg\n.*ifge.*\n.*iinc\s+2, 1", binomial):
        raise ValueError("binomial_with_bonus_count is no longer level + extra rounds of nextFloat < p")
    table = method(inspect("net.minecraft.world.level.storage.loot.predicates.BonusLevelTableCondition"),
                   "public boolean test(net.minecraft.world.level.storage.loot.LootContext);")
    if not re.search(r"List\.size:\(\)I\n.*iconst_1\n.*isub\n.*Math\.min:\(II\)I(?:.*\n){1,8}.*nextFloat:\(\)F\n"
                     r".*fload\s+4\n.*fcmpg\n.*ifge", table):
        raise ValueError("table_bonus is no longer nextFloat < chances[min(level, size - 1)]")
    count = method(inspect("net.minecraft.world.level.storage.loot.functions.SetItemCountFunction"),
                   "public net.minecraft.world.item.ItemStack run(")
    if not re.search(r"add:Z\n.*ifeq(?:.*\n){1,4}.*iconst_0\n.*istore_3(?:.*\n){1,8}.*getInt:.*\n.*iadd\n"
                     r".*ItemStack\.setCount", count):
        raise ValueError("set_count is no longer (add ? count : 0) + provider")
    ints = "net.minecraft.world.level.storage.loot.providers.number.ints."
    uniform_int = method(inspect(ints + "UniformGenerator"), "public int getIntUnsafe(")
    if "Mth.nextInt:(Lnet/minecraft/util/RandomSource;II)I" not in uniform_int:
        raise ValueError("uniform count provider no longer uses Mth.nextInt(min, max)")
    binomial_int = method(inspect(ints + "BinomialDistributionGenerator"), "public int getIntUnsafe(")
    if not re.search(r"nextFloat:\(\)F\n.*fload_3\n.*fcmpg\n.*ifge", binomial_int):
        raise ValueError("binomial count provider is no longer n rounds of nextFloat < p")
    pot = method(inspect("net.minecraft.world.level.block.DecoratedPotBlock"),
                 "public net.minecraft.world.level.block.state.BlockState playerWillDestroy(")
    if not re.search(r"ItemTags\.BREAKS_DECORATED_POTS(?:.*\n){1,3}.*ifeq(?:.*\n){1,4}"
                     r".*EnchantmentTags\.PREVENTS_DECORATED_POT_SHATTERING(?:.*\n){1,3}.*ifne(?:.*\n){1,4}"
                     r".*CRACKED", pot):
        raise ValueError("DecoratedPotBlock no longer cracks pots for #breaks_decorated_pots tools")
    decorations = inspect("net.minecraft.world.level.block.entity.PotDecorations")
    if "ItemIds.BRICK" not in decorations:
        raise ValueError("an undecorated pot side is no longer a brick")


def table_redirects(inspect, tables):
    """Blocks without a table file: wall variants and the blocks that drop nothing."""
    names = {}
    for holder in ("BlockIds", "BlockItemIds"):
        init = inspect(f"net.minecraft.references.{holder}").split("  static {};")[1]
        for body, field in re.findall(r"((?:.*\n)*?).*putstatic\s+#\d+\s+// Field (\w+):Lnet/minecraft/"
                                      r"(?:resources/ResourceKey|references/BlockItemId);", init):
            strings = re.findall(r"// String ([a-z0-9_]+)\n", body)
            if strings:
                names[(holder, field)] = strings[0]
    code = inspect("net.minecraft.world.level.block.Blocks")
    init = code.split("  static {};")[1]
    segments = re.split(r"(putstatic\s+#\d+\s+// Field \w+:Lnet/minecraft/world/level/block/"
                        r"(?:Block|ColorCollection);)", init)
    field_ids = {}
    found = {}
    for body, store in zip(segments[0::2], segments[1::2]):
        field = re.search(r"Field (\w+):", store).group(1)
        ids = re.findall(r"Field net/minecraft/references/(Block(?:Item)?Ids)\.(\w+):Lnet/minecraft/"
                         r"(?:resources/ResourceKey|references/BlockItemId);", body)
        if not ids:
            continue
        block = names[ids[0]]
        field_ids[field] = block
        if block in tables:
            continue
        call = body.find("Method wallVariant:")
        base = None
        if call >= 0:
            bases = re.findall(r"getstatic\s+#\d+\s+// Field (\w+):Lnet/minecraft/world/level/block/Block;",
                               body[:call])
            base = field_ids[bases[-1]]
        found[block] = {"loot_table": None if base is None else f"minecraft:{base}",
                        "source": "Blocks.wallVariant (overrideLootTable)" if base else
                        ("noLootTable" if "noLootTable" in body else "no table file")}
    banner = re.search(r"getstatic\s+#\d+\s+// Field BANNER:Lnet/minecraft/world/level/block/ColorCollection;\n"
                       r".*aload_0\n.*ColorCollection\.pick(?:.*\n){1,3}.*Method wallVariant:", code)
    if not banner or "Field WALL_BANNER:Lnet/minecraft/world/level/block/ColorCollection;" not in init:
        raise ValueError("wall banners no longer take their banner's table")
    for color in COLORS:
        found[f"{color}_wall_banner"] = {"loot_table": f"minecraft:{color}_banner",
                                         "source": "Blocks.wallVariant (overrideLootTable)"}
    for block, info in found.items():
        if info["loot_table"] and info["loot_table"].removeprefix("minecraft:") not in tables:
            raise ValueError(f"{block} redirects to a missing table {info['loot_table']}")
    return found


class Table:
    """Exact evaluation of one block loot table."""

    def __init__(self, block, raw, predicates, tags):
        self.block = f"minecraft:{block}"
        self.raw = raw
        self.predicates = predicates
        self.tags = tags
        self.properties = {}
        self.notes = set()
        self.explosion = set()
        if set(raw) - {"type", "pools", "random_sequence", "modifier"} or raw["type"] != "minecraft:block":
            raise ValueError(f"unrecognized table shape: {block}")
        for function in as_list(raw.get("modifier")):
            self.scan_function(function)
        for pool in raw.get("pools", []):
            if set(pool) - {"entries", "rolls", "condition", "modifier"} or pool["rolls"] != 1:
                raise ValueError(f"unrecognized pool in {block}: {sorted(pool)}")
            for c in as_list(pool.get("condition")):
                self.scan_condition(c)
            for function in as_list(pool.get("modifier")):
                self.scan_function(function)
            for entry in pool["entries"]:
                self.scan_entry(entry)
        if len(raw.get("pools", [])) and any(len(p["entries"]) != 1 for p in raw["pools"]):
            raise ValueError(f"weighted pool choice is not modelled: {block}")

    # Shape checks and the property values the conditions name.
    def scan_condition(self, c):
        if isinstance(c, str):
            if c not in self.predicates:
                raise ValueError(f"unknown predicate reference {c} in {self.block}")
            return
        kind = c.get("type")
        if kind == "minecraft:match_block":
            if set(c) != {"type", "blocks", "state"} or c["blocks"] != self.block:
                raise ValueError(f"unrecognized match_block in {self.block}: {c}")
            for prop, value in c["state"].items():
                if not isinstance(value, str):
                    raise ValueError(f"ranged state match in {self.block}: {c}")
                self.properties.setdefault(prop, set()).add(value)
        elif kind == "minecraft:survives_explosion":
            self.explosion.add("survives_explosion")
        elif kind == "minecraft:table_bonus":
            if c.get("enchantment") != "minecraft:fortune" or set(c) != {"type", "chances", "enchantment"}:
                raise ValueError(f"unrecognized table_bonus in {self.block}: {c}")
        elif kind == "minecraft:random_chance":
            if set(c) != {"type", "chance"} or not isinstance(c["chance"], (int, float)):
                raise ValueError(f"unrecognized random_chance in {self.block}: {c}")
        elif kind == "minecraft:match_tool":
            if c != {"type": kind, "predicate": {"items": "#minecraft:cluster_max_harvestables"}}:
                raise ValueError(f"unrecognized match_tool in {self.block}: {c}")
        elif kind == "minecraft:location_check":
            other = c.get("predicate", {}).get("block", {})
            if set(c) != {"type", "offsetY", "predicate"} or other.get("blocks") != self.block \
                    or set(other.get("state", {})) != {"half"}:
                raise ValueError(f"unrecognized location_check in {self.block}: {c}")
            self.notes.add("location_check: the other half of the double block is present")
        elif kind == "minecraft:entity_properties":
            if c != {"type": kind, "entity": "this", "predicate": {}}:
                raise ValueError(f"unrecognized entity_properties in {self.block}: {c}")
            self.notes.add("entity_properties this: drops only when an entity (the player) breaks it")
        elif kind == "minecraft:inverted":
            self.scan_condition(c["term"])
        elif kind in ("minecraft:any_of", "minecraft:all_of"):
            for term in c["terms"]:
                self.scan_condition(term)
        else:
            raise ValueError(f"unknown condition {kind} in {self.block}")

    def scan_function(self, f):
        kind = f.get("type")
        keys = set(f) - {"condition"}
        for c in as_list(f.get("condition")):
            self.scan_condition(c)
        if kind == "minecraft:set_count":
            if keys - {"type", "count", "add"}:
                raise ValueError(f"unrecognized set_count in {self.block}: {f}")
            self.provider(f["count"])
        elif kind == "minecraft:apply_bonus":
            formula = f.get("formula")
            if f.get("enchantment") != "minecraft:fortune" or formula not in (
                    "minecraft:ore_drops", "minecraft:uniform_bonus_count", "minecraft:binomial_with_bonus_count"):
                raise ValueError(f"unrecognized apply_bonus in {self.block}: {f}")
        elif kind == "minecraft:limit_count":
            if keys != {"type", "limit"} or set(f["limit"]) - {"min", "max"}:
                raise ValueError(f"unrecognized limit_count in {self.block}: {f}")
        elif kind == "minecraft:explosion_decay":
            self.explosion.add("explosion_decay")
        elif kind == "minecraft:copy_components":
            self.notes.add("copy_components: " + ", ".join(f["include"]))
        elif kind == "minecraft:copy_state":
            self.notes.add("copy_state: " + ", ".join(f["properties"]))
        else:
            raise ValueError(f"unknown function {kind} in {self.block}")

    def scan_entry(self, e):
        kind = e.get("type")
        for c in as_list(e.get("condition")):
            self.scan_condition(c)
        if kind == "minecraft:item":
            if set(e) - {"type", "name", "condition", "modifier"}:
                raise ValueError(f"unrecognized item entry in {self.block}: {e}")
            for f in as_list(e.get("modifier")):
                self.scan_function(f)
        elif kind == "minecraft:alternatives":
            if set(e) - {"type", "children", "condition"}:
                raise ValueError(f"unrecognized alternatives in {self.block}: {e}")
            for child in e["children"]:
                self.scan_entry(child)
        elif kind == "minecraft:dynamic":
            if e.get("name") != "minecraft:sherds" or self.block != "minecraft:decorated_pot":
                raise ValueError(f"unrecognized dynamic entry in {self.block}: {e}")
            self.notes.add("dynamic sherds: an undecorated pot's four sides are bricks")
        else:
            raise ValueError(f"unknown entry {kind} in {self.block}")

    @staticmethod
    def provider(count):
        if isinstance(count, int):
            return
        kind = count.get("type") if isinstance(count, dict) else None
        if kind == "minecraft:uniform" and set(count) == {"type", "min", "max"} \
                and all(isinstance(count[k], int) for k in ("min", "max")):
            return
        if kind == "minecraft:binomial" and set(count) == {"type", "n", "p"} and isinstance(count["n"], int):
            return
        raise ValueError(f"unrecognized count provider: {count}")

    # Probabilities.
    def chance(self, c, state, tool):
        if isinstance(c, str):
            return self.chance(self.predicates[c], state, tool)
        kind = c["type"]
        if kind == "minecraft:match_tool":
            predicate = c["predicate"]
            if predicate == {"items": "minecraft:shears"}:
                return float(tool["shears"])
            if predicate == {"items": "#minecraft:cluster_max_harvestables"}:
                return float(tool["pickaxe"])
            if predicate == {"predicates": {"minecraft:enchantments": [
                    {"enchantments": "minecraft:silk_touch", "levels": {"min": 1}}]}}:
                return float(tool["silk_touch"])
            raise ValueError(f"unknown tool predicate {predicate}")
        if kind == "minecraft:match_block":
            return float(all(state.get(k) == v for k, v in c["state"].items()))
        if kind in ("minecraft:survives_explosion", "minecraft:location_check", "minecraft:entity_properties"):
            return 1.0
        if kind == "minecraft:table_bonus":
            return c["chances"][min(tool["fortune"], len(c["chances"]) - 1)]
        if kind == "minecraft:random_chance":
            return float(c["chance"])
        if kind == "minecraft:inverted":
            return 1.0 - self.chance(c["term"], state, tool)
        if kind == "minecraft:all_of":
            p = 1.0
            for term in c["terms"]:
                p *= self.chance(term, state, tool)
            return p
        if kind == "minecraft:any_of":
            q = 1.0
            for term in c["terms"]:
                q *= 1.0 - self.chance(term, state, tool)
            return 1.0 - q
        raise ValueError(kind)

    def conditions(self, holder, state, tool):
        p = 1.0
        for c in as_list(holder.get("condition")):
            p *= self.chance(c, state, tool)
        return p

    def apply(self, dist, f, state, tool):
        """One function over a count distribution `{count: probability}`."""
        p = self.conditions(f, state, tool)
        if p <= 0.0:
            return dist
        kind = f["type"]
        out = {}

        def add(count, weight):
            out[count] = out.get(count, 0.0) + weight

        for count, weight in dist.items():
            if kind == "minecraft:set_count":
                base = count if f.get("add") else 0
                for value, w in provider_dist(f["count"]).items():
                    add(base + value, weight * w)
            elif kind == "minecraft:apply_bonus":
                for value, w in bonus_dist(f, count, tool["fortune"]).items():
                    add(value, weight * w)
            elif kind == "minecraft:limit_count":
                limit = f["limit"]
                add(min(max(count, limit.get("min", count)), limit.get("max", count)), weight)
            else:  # explosion_decay, copy_components, copy_state keep the count when mining.
                add(count, weight)
        if p < 1.0:
            mixed = {c: w * p for c, w in out.items()}
            for count, weight in dist.items():
                mixed[count] = mixed.get(count, 0.0) + weight * (1.0 - p)
            return mixed
        return out

    def item_outcome(self, entry, functions, state, tool):
        """`(item, count distribution)` of an item entry that runs."""
        if entry["type"] == "minecraft:dynamic":
            return "minecraft:brick", {4: 1.0}
        dist = {1: 1.0}
        for f in as_list(entry.get("modifier")) + functions:
            dist = self.apply(dist, f, state, tool)
        return entry["name"], dist

    def expand(self, entry, functions, state, tool):
        """`[(probability, item, distribution)]` for an entry; the rest is failure."""
        p = self.conditions(entry, state, tool)
        if p <= 0.0:
            return []
        if entry["type"] in ("minecraft:item", "minecraft:dynamic"):
            item, dist = self.item_outcome(entry, functions, state, tool)
            return [(p, item, dist)]
        results = []
        remaining = p
        for child in entry["children"]:
            outcomes = self.expand(child, functions, state, tool)
            success = sum(w for w, _, _ in outcomes)
            results += [(remaining * w, item, dist) for w, item, dist in outcomes]
            remaining *= 1.0 - success
            if remaining <= EPSILON:
                break
        return results

    def drops(self, state, tool):
        totals = {}
        table_functions = as_list(self.raw.get("modifier"))
        for pool in self.raw.get("pools", []):
            p = self.conditions(pool, state, tool)
            if p <= 0.0:
                continue
            functions = as_list(pool.get("modifier")) + table_functions
            per_item = {}
            for w, item, dist in self.expand(pool["entries"][0], functions, state, tool):
                share = per_item.setdefault(item, {})
                for count, weight in dist.items():
                    if count > 0:
                        share[count] = share.get(count, 0.0) + p * w * weight
            for item, share in per_item.items():
                absent = 1.0 - sum(share.values())
                pool_dist = dict(share)
                pool_dist[0] = pool_dist.get(0, 0.0) + absent
                totals[item] = convolve(totals.get(item, {0: 1.0}), pool_dist)
        rows = []
        for item, dist in sorted(totals.items()):
            present = {c: w for c, w in dist.items() if c > 0 and w > EPSILON}
            if not present:
                continue
            mean = sum(c * w for c, w in dist.items())
            var = sum(c * c * w for c, w in dist.items()) - mean * mean
            rows.append([item, min(present), max(present), rounded(sum(present.values())), rounded(mean),
                         rounded(max(var, 0.0))])
        return rows


def convolve(a, b):
    out = {}
    for x, wx in a.items():
        for y, wy in b.items():
            out[x + y] = out.get(x + y, 0.0) + wx * wy
    return out


def rounded(value):
    return float(f"{value:.9g}")


def provider_dist(count):
    if isinstance(count, int):
        return {count: 1.0}
    if count["type"] == "minecraft:uniform":
        lo, hi = count["min"], count["max"]
        if lo >= hi:
            return {lo: 1.0}
        return {v: 1.0 / (hi - lo + 1) for v in range(lo, hi + 1)}
    return binomial(count["n"], count["p"])


def binomial(n, p):
    dist = {0: 1.0}
    for _ in range(n):
        dist = convolve(dist, {0: 1.0 - p, 1: p})
    return dist


def bonus_dist(f, count, level):
    formula = f["formula"]
    if formula == "minecraft:ore_drops":
        if level <= 0:
            return {count: 1.0}
        out = {}
        for roll in range(level + 2):
            times = max(roll - 1, 0) + 1
            out[count * times] = out.get(count * times, 0.0) + 1.0 / (level + 2)
        return out
    if formula == "minecraft:uniform_bonus_count":
        bound = f["parameters"]["bonusMultiplier"] * level + 1
        return {count + i: 1.0 / bound for i in range(bound)}
    parameters = f["parameters"]
    return {count + k: w for k, w in binomial(level + parameters["extra"], parameters["probability"]).items()}


def cases(table):
    """Every combination of named property values, with `*` for the rest."""
    keys = [{}]
    for prop in sorted(table.properties):
        values = sorted(table.properties[prop], key=lambda v: (len(v), v)) + ["*"]
        keys = [dict(k, **{prop: v}) for k in keys for v in values]
    return keys


def case_key(state):
    return ",".join(f"{k}={v}" for k, v in sorted(state.items()))


def evaluate(table):
    out = {}
    for state in cases(table):
        scenarios = {name: table.drops(state, tool) for name, tool in SCENARIOS.items()}
        base = scenarios["pickaxe"]
        out[case_key(state)] = {"pickaxe": base} | {
            name: rows for name, rows in scenarios.items() if name != "pickaxe" and rows != base}
    return out


def expand_item_tag(read, name, seen=()):
    if name in seen:
        raise ValueError(f"recursive tag {name}")
    namespace, path = name.split(":")
    values = []
    for value in read(f"data/{namespace}/tags/item/{path}.json")["values"]:
        if not isinstance(value, str):
            raise ValueError(f"unrecognized tag value in {name}: {value}")
        values += expand_item_tag(read, value[1:], seen + (name,)) if value.startswith("#") else [value]
    return sorted(values)


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def read(name):
            return json.loads(jar.read(name))

        names = sorted(n[len(ROOT):-5] for n in jar.namelist() if n.startswith(ROOT) and n.endswith(".json"))
        raw = {name: read(f"{ROOT}{name}.json") for name in names}
        predicates = {f"minecraft:tool/{name}": read(f"data/minecraft/predicate/tool/{name}.json")
                      for name in ("can_shear", "can_silk_touch")}
        if predicates["minecraft:tool/can_shear"] != {"type": "minecraft:match_tool",
                                                       "predicate": {"items": "minecraft:shears"}}:
            raise ValueError("tool/can_shear is no longer shears")
        tags = {name: expand_item_tag(read, f"minecraft:{name}")
                for name in ("cluster_max_harvestables", "breaks_decorated_pots", "pickaxes")}
        if tags["cluster_max_harvestables"] != tags["pickaxes"]:
            raise ValueError("#cluster_max_harvestables is no longer exactly the pickaxes")
        shatter = read("data/minecraft/tags/enchantment/prevents_decorated_pot_shattering.json")["values"]
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-block-loot-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(["javap", "-classpath", str(path), "-c", "-p", name], text=True)

        check_bytecode(inspect)
        redirects = table_redirects(inspect, set(names))
    blocks = {}
    for name in names:
        table = Table(name, raw[name], predicates, tags)
        entry = {"properties": {k: sorted(v, key=lambda v: (len(v), v)) for k, v in sorted(table.properties.items())},
                 "explosion": sorted(table.explosion), "cases": evaluate(table)}
        if table.notes:
            entry["notes"] = sorted(table.notes)
        blocks[f"minecraft:{name}"] = entry
    for name, info in sorted(redirects.items()):
        if info["loot_table"]:
            blocks[f"minecraft:{name}"] = {"loot_table": info["loot_table"], "source": info["source"]}
        else:
            blocks[f"minecraft:{name}"] = {"loot_table": None, "source": info["source"], "properties": {},
                                           "explosion": [], "cases": {"": {"pickaxe": []}}}
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            f"{ROOT}*.json ({len(names)} tables)",
            "data/minecraft/predicate/tool/can_shear.json, can_silk_touch.json",
            "data/minecraft/tags/item/cluster_max_harvestables.json, breaks_decorated_pots.json (expanded)",
            "data/minecraft/tags/enchantment/prevents_decorated_pot_shattering.json",
            "net.minecraft.world.level.block.Blocks (wallVariant, noLootTable)",
            "net.minecraft.world.level.storage.loot.functions.ApplyBonusCount$OreDrops, $UniformBonusCount, "
            "$BinomialWithBonusCount; SetItemCountFunction",
            "net.minecraft.world.level.storage.loot.predicates.BonusLevelTableCondition",
            "net.minecraft.world.level.storage.loot.providers.number.ints.UniformGenerator, "
            "BinomialDistributionGenerator",
            "net.minecraft.world.level.block.DecoratedPotBlock.playerWillDestroy; "
            "net.minecraft.world.level.block.entity.PotDecorations",
        ],
        "scenarios": SCENARIOS,
        "row": ["item", "min", "max", "chance", "mean", "variance"],
        "semantics": [
            "cases: block-state values named by the table's conditions; '*' is any other value",
            "each case lists 'pickaxe' and only the scenarios whose drops differ from it",
            "chance: probability of at least one; min/max: over counts above zero; mean/variance include zero",
            "mining: survives_explosion passes and explosion_decay keeps every item",
            "requiresCorrectToolForDrops is outside the tables: a scenario applies only to a tool that harvests",
            "a decorated pot broken by a #breaks_decorated_pots tool without Silk Touch is cracked first",
        ],
        "tags": tags | {"prevents_decorated_pot_shattering": shatter},
        "blocks": dict(sorted(blocks.items())),
    }


def dump(fixture):
    """Stable JSON with one line per case, so diffs stay readable."""
    head = {k: v for k, v in fixture.items() if k != "blocks"}
    text = json.dumps(head, indent=1)[:-2] + ',\n "blocks": {\n'
    lines = []
    for name, block in fixture["blocks"].items():
        if "cases" not in block:
            lines.append(f"  {json.dumps(name)}: {json.dumps(block)}")
            continue
        fields = [f"   {json.dumps(k)}: {json.dumps(v)}" for k, v in block.items() if k != "cases"]
        cases_text = ",\n".join(f"    {json.dumps(k)}: {json.dumps(v)}" for k, v in block["cases"].items())
        fields.append('   "cases": {\n' + cases_text + "\n   }")
        lines.append(f"  {json.dumps(name)}: {{\n" + ",\n".join(fields) + "\n  }")
    return text + ",\n".join(lines) + "\n }\n}\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        fixture = extract(args.server)
    except (OSError, ValueError, KeyError, zipfile.BadZipFile, subprocess.CalledProcessError) as error:
        parser.error(str(error))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(dump(fixture), encoding="utf-8")


if __name__ == "__main__":
    main()
