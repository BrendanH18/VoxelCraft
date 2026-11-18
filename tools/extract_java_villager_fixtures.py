#!/usr/bin/env python3
"""Extract the v0.15 Track H villager facts from the pinned Java 26.3 server.

Trades are data in 26.3. For every profession level and the wandering
trader this reads the trade set (`data/minecraft/trade_set`: how many offers
are drawn, whether duplicates are allowed), resolves its
`#villager_trade/...` tag in file order (nested tags such as
`#common_smith/level_1` expanded where they appear), and normalizes each
`data/minecraft/villager_trade` entry with the codec defaults of
`VillagerTrade.CODEC` (`max_uses` 4, `xp` 1, `reputation_discount` 0,
counts 1). Only the default data is read; the optional `trade_rebalance`
data pack is listed by name but not applied, as 26.3 ships it disabled.

It also records the enchantment and potion tags the trades draw from, the villager
schedules (`data/minecraft/timeline/villager_schedule.json`) and constants
read from the classes with javap: `Villager`, `VillagerData`'s level
thresholds, `GossipType`, `PoiTypes`' tickets and ranges, `AcquirePoi`,
`WorkAtPoi`, `GolemSensor`, `ZombieVillager`, `WanderingTraderSpawner`, the
wandering trader's avoid goals and trade-set order, the villager hostile
sensor's distances and the food values (`Item.Properties.villagerFood`).

The game is compared with the fixture in `tests/java_villagers.rs`.
Requires javap. Unknown data or bytecode forms fail rather than
regenerating a guessed fixture. No VoxelCraft source is consulted.
"""

import argparse
import hashlib
import io
import json
import re
import subprocess
import tempfile
import zipfile
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

PROFESSIONS = [
    "armorer",
    "butcher",
    "cartographer",
    "cleric",
    "farmer",
    "fisherman",
    "fletcher",
    "leatherworker",
    "librarian",
    "mason",
    "shepherd",
    "toolsmith",
    "weaponsmith",
]
TRADE_KEYS = {
    "wants",
    "additional_wants",
    "gives",
    "max_uses",
    "xp",
    "reputation_discount",
    "merchant_predicate",
    "given_item_modifier",
    "double_trade_price_enchantments",
}
COST_KEYS = {"id", "count", "components"}
ENCHANTMENT_TAGS = ["tradeable", "on_traded_equipment", "double_trade_price"]
# Named constants: (class under net.minecraft, field).
CONSTANTS = [
    ("world.entity.npc.villager.Villager", "BREEDING_FOOD_THRESHOLD"),
    ("world.entity.npc.villager.Villager", "MAX_GOSSIP_TOPICS"),
    ("world.entity.npc.villager.Villager", "GOSSIP_COOLDOWN"),
    ("world.entity.npc.villager.Villager", "GOSSIP_DECAY_INTERVAL"),
    ("world.entity.npc.villager.Villager", "HOW_FAR_AWAY_TO_TALK_TO_OTHER_VILLAGERS_ABOUT_GOLEMS"),
    ("world.entity.npc.villager.Villager", "HOW_MANY_VILLAGERS_NEED_TO_AGREE_TO_SPAWN_A_GOLEM"),
    ("world.entity.npc.villager.Villager", "TIME_SINCE_SLEEPING_FOR_GOLEM_SPAWNING"),
    ("world.entity.ai.gossip.GossipType", "REPUTATION_CHANGE_PER_EVENT"),
    ("world.entity.ai.gossip.GossipType", "REPUTATION_CHANGE_PER_EVERLASTING_MEMORY"),
    ("world.entity.ai.gossip.GossipType", "REPUTATION_CHANGE_PER_TRADE"),
    ("world.entity.ai.gossip.GossipContainer", "DISCARD_THRESHOLD"),
    ("world.entity.ai.behavior.AcquirePoi", "SCAN_RANGE"),
    ("world.entity.ai.behavior.WorkAtPoi", "CHECK_COOLDOWN"),
    ("world.entity.ai.behavior.WorkAtPoi", "DISTANCE"),
    ("world.entity.ai.sensing.GolemSensor", "GOLEM_SCAN_RATE"),
    ("world.entity.ai.sensing.GolemSensor", "MEMORY_TIME_TO_LIVE"),
    ("world.entity.monster.zombie.ZombieVillager", "VILLAGER_CONVERSION_WAIT_MIN"),
    ("world.entity.monster.zombie.ZombieVillager", "VILLAGER_CONVERSION_WAIT_MAX"),
    ("world.entity.monster.zombie.ZombieVillager", "MAX_SPECIAL_BLOCKS_COUNT"),
    ("world.entity.monster.zombie.ZombieVillager", "SPECIAL_BLOCK_RADIUS"),
    ("world.entity.npc.wanderingtrader.WanderingTraderSpawner", "DEFAULT_TICK_DELAY"),
    ("world.entity.npc.wanderingtrader.WanderingTraderSpawner", "DEFAULT_SPAWN_DELAY"),
    ("world.entity.npc.wanderingtrader.WanderingTraderSpawner", "MIN_SPAWN_CHANCE"),
    ("world.entity.npc.wanderingtrader.WanderingTraderSpawner", "MAX_SPAWN_CHANCE"),
    ("world.entity.npc.wanderingtrader.WanderingTraderSpawner", "SPAWN_CHANCE_INCREASE"),
    ("world.entity.npc.wanderingtrader.WanderingTraderSpawner", "SPAWN_ONE_IN_X_CHANCE"),
    ("world.entity.npc.wanderingtrader.WanderingTraderSpawner", "NUMBER_OF_SPAWN_ATTEMPTS"),
]
# Inner-class constants javap names with `$`.
INNER_CONSTANTS = [
    ("world.entity.ai.behavior.AcquirePoi$JitteredLinearRetry", "MIN_INTERVAL_INCREASE"),
    ("world.entity.ai.behavior.AcquirePoi$JitteredLinearRetry", "MAX_INTERVAL_INCREASE"),
    ("world.entity.ai.behavior.AcquirePoi$JitteredLinearRetry", "MAX_RETRY_PATHFINDING_INTERVAL"),
]


def strip(name):
    if not isinstance(name, str) or not name.startswith("minecraft:"):
        raise ValueError(f"not a minecraft id: {name!r}")
    return name[len("minecraft:") :]


def unspace(value):
    """Drop the `minecraft:` namespace from every string, keys included."""
    if isinstance(value, str):
        if value.startswith("#minecraft:"):
            return "#" + value[len("#minecraft:") :]
        return value[len("minecraft:") :] if value.startswith("minecraft:") else value
    if isinstance(value, list):
        return [unspace(v) for v in value]
    if isinstance(value, dict):
        return {unspace(k): unspace(v) for k, v in value.items()}
    return value


def read_json(jar, path):
    return json.loads(jar.read(path))


def resolve_tag(jar, root, name, seen=()):
    """Tag values in file order, nested `#` tags expanded where they appear."""
    if name in seen:
        raise ValueError(f"tag cycle at {root}/{name}")
    out = []
    for v in read_json(jar, f"data/minecraft/tags/{root}/{name}.json")["values"]:
        if not isinstance(v, str):
            raise ValueError(f"unexpected tag entry {v!r} in {root}/{name}")
        if v.startswith("#"):
            out.extend(resolve_tag(jar, root, strip(v[1:]), seen + (name,)))
        else:
            out.append(strip(v))
    return out


def cost(raw, where):
    if set(raw) - COST_KEYS:
        raise ValueError(f"{where}: unknown cost keys {sorted(set(raw) - COST_KEYS)}")
    count = raw.get("count", 1)
    if not isinstance(count, int):
        raise ValueError(f"{where}: count provider {count!r}")
    out = {"item": strip(raw["id"]), "count": count}
    if "components" in raw:
        out["components"] = unspace(raw["components"])
    return out


def villager_types(predicate, where):
    """`entity_properties` on `this` testing `villager/variant`."""
    if predicate.get("type") != "minecraft:entity_properties" or predicate.get("entity") != "this":
        raise ValueError(f"{where}: unexpected merchant predicate {predicate!r}")
    inner = predicate["predicate"]
    if list(inner) != ["minecraft:predicates"] or list(inner["minecraft:predicates"]) != ["minecraft:villager/variant"]:
        raise ValueError(f"{where}: unexpected merchant predicate {predicate!r}")
    types = inner["minecraft:predicates"]["minecraft:villager/variant"]
    types = types if isinstance(types, list) else [types]
    return [strip(t) for t in types]


def scalar(value, where, kind):
    if not isinstance(value, kind) or isinstance(value, bool):
        raise ValueError(f"{where}: provider {value!r}")
    return value


def trade(jar, trade_id):
    where = f"villager_trade/{trade_id}"
    raw = read_json(jar, f"data/minecraft/{where}.json")
    if set(raw) - TRADE_KEYS:
        raise ValueError(f"{where}: unknown keys {sorted(set(raw) - TRADE_KEYS)}")
    gives = raw["gives"]
    modifiers = raw.get("given_item_modifier", [])
    if set(gives) - {"id", "count"}:
        raise ValueError(f"{where}: unknown gives keys")
    out = {
        "wants": cost(raw["wants"], where),
        "additional_wants": cost(raw["additional_wants"], where) if "additional_wants" in raw else None,
        "gives": {"item": strip(gives["id"]), "count": scalar(gives.get("count", 1), where, int)},
        # `VillagerTrade.CODEC` defaults.
        "max_uses": scalar(raw.get("max_uses", 4), where, int),
        "xp": scalar(raw.get("xp", 1), where, int),
        "reputation_discount": scalar(raw.get("reputation_discount", 0.0), where, (int, float)),
        "villager_types": villager_types(raw["merchant_predicate"], where) if "merchant_predicate" in raw else None,
        # A single function or a list of them (`LootItemFunctions.CODEC`).
        "modifiers": unspace(modifiers if isinstance(modifiers, list) else [modifiers]),
        "double_trade_price_enchantments": unspace(raw.get("double_trade_price_enchantments")),
    }
    for m in out["modifiers"]:
        if m["type"] not in {
            "enchant_with_levels",
            "enchant_randomly",
            "exploration_map",
            "filtered",
            "set_potion",
            "set_random_dyes",
            "set_random_potion",
            "set_stew_effect",
        }:
            raise ValueError(f"{where}: unknown modifier {m['type']}")
    return out


def trade_set(jar, path):
    raw = read_json(jar, f"data/minecraft/trade_set/{path}.json")
    if set(raw) - {"amount", "random_sequence", "trades", "allow_duplicates"}:
        raise ValueError(f"trade_set/{path}: unknown keys")
    if raw.get("random_sequence") != f"minecraft:trade_set/{path}":
        raise ValueError(f"trade_set/{path}: unexpected random sequence")
    tag = raw["trades"]
    if not tag.startswith("#minecraft:"):
        raise ValueError(f"trade_set/{path}: trades are not a tag")
    return {
        "amount": scalar(raw["amount"], path, int),
        "allow_duplicates": raw.get("allow_duplicates", False),
        "trades": resolve_tag(jar, "villager_trade", strip(tag[1:])),
    }


def operand(line):
    """The int or float a javap instruction pushes, else None."""
    m = re.search(r"\b(iconst|lconst|fconst)_(m1|\d)\b", line)
    if m:
        return -1 if m[2] == "m1" else int(m[2])
    m = re.search(r"\b(?:bipush|sipush)\s+(-?\d+)", line)
    if m:
        return int(m[1])
    m = re.search(r"// (?:int|long|float|double) (-?[0-9.E]+)[lfd]?$", line.strip())
    if m:
        v = m[1]
        return float(v) if ("." in v or "E" in v) else int(v)
    return None


def method(text, signature_start):
    """The bytecode lines of the first method whose header starts with `signature_start`."""
    lines = text.splitlines()
    for i, line in enumerate(lines):
        if line.strip().startswith(signature_start) and i + 1 < len(lines) and lines[i + 1].strip() == "Code:":
            body = []
            for code in lines[i + 2 :]:
                if not re.match(r"\s+\d+: ", code):
                    break
                body.append(code)
            return body
    raise ValueError(f"missing method {signature_start}")


def named_constant(text, cls, name):
    m = re.search(r"static final (?:int|long|float|double) " + name + r" = (-?[0-9.E]+)[lfdL]?;", text)
    if not m:
        raise ValueError(f"missing {cls}.{name}")
    v = m[1]
    return float(v) if ("." in v or "E" in v) else int(v)


def xp_thresholds(text):
    """`NEXT_LEVEL_XP_THRESHOLDS`: `dup`, index, value, `iastore` per element."""
    lines = method(text, "static {}")
    start = next(i for i, line in enumerate(lines) if "newarray" in line and "int" in line)
    values = []
    pushed = []
    for line in lines[start + 1 :]:
        if "putstatic" in line:
            if "NEXT_LEVEL_XP_THRESHOLDS" not in line:
                raise ValueError("unexpected VillagerData array")
            break
        if "iastore" in line:
            if len(pushed) != 2 or pushed[0] != len(values):
                raise ValueError("unrecognized VillagerData array store")
            values.append(pushed[1])
            pushed = []
            continue
        v = operand(line)
        if v is not None:
            pushed.append(v)
    return values


def gossip_types(text):
    out = []
    lines = method(text, "static {}")
    i = 0
    while i < len(lines):
        m = re.search(r"// String ([a-z_]+)$", lines[i])
        if m:
            ints = []
            j = i + 1
            while "invokespecial" not in lines[j]:
                v = operand(lines[j])
                if v is not None:
                    ints.append(v)
                j += 1
            if len(ints) != 4:
                raise ValueError(f"unexpected GossipType {m[1]}")
            out.append(
                {"id": m[1], "weight": ints[0], "max": ints[1], "decay_per_day": ints[2], "decay_per_transfer": ints[3]}
            )
            i = j
        i += 1
    if not out:
        raise ValueError("no gossip types")
    return out


def poi_types(text):
    out = {}
    lines = method(text, "public static net.minecraft.world.entity.ai.village.poi.PoiType bootstrap(")
    key = None
    ints = []
    for line in lines:
        m = re.search(r"getstatic .*// Field ([A-Z_]+):Lnet/minecraft/resources/ResourceKey;", line)
        if m:
            key, ints = m[1].lower(), []
            continue
        v = operand(line)
        if v is not None:
            ints.append(v)
        if "Method register:" in line:
            if key is None or len(ints) != 2:
                raise ValueError(f"unexpected PoiTypes registration {key}")
            out[key] = {"max_tickets": ints[0], "valid_range": ints[1]}
            key = None
    return out


def hostile_distances(text):
    out = {}
    entity = None
    for line in method(text, "static {}"):
        m = re.search(r"Field net/minecraft/world/entity/EntityTypes\.([A-Z_]+):", line)
        if m:
            entity = m[1].lower()
            continue
        v = operand(line)
        if v is not None and entity:
            out[entity] = v
            entity = None
    if not out:
        raise ValueError("no villager hostile distances")
    return out


def trader_goals(text):
    """`AvoidEntityGoal(this, Class, distance, walk, sprint)` and the trade-set order."""
    avoid = []
    lines = method(text, "protected void registerGoals()")
    for i, line in enumerate(lines):
        m = re.search(r"// class net/minecraft/world/entity/(?:[a-z]+/)*([A-Za-z]+)$", line)
        if m and m[1] != "AvoidEntityGoal" and "AvoidEntityGoal" in "".join(lines[i : i + 6]):
            nums = [operand(x) for x in lines[i + 1 : i + 4]]
            avoid.append({"class": m[1], "distance": nums[0], "walk": nums[1], "sprint": nums[2]})
    order = re.findall(
        r"Field net/minecraft/world/item/trading/TradeSets\.WANDERING_TRADER_([A-Z]+):",
        "\n".join(method(text, "protected void updateTrades(")),
    )
    if not avoid or not order:
        raise ValueError("unrecognized WanderingTrader goals")
    return avoid, [o.lower() for o in order]


def villager_food(items):
    out = {}
    lines = items.splitlines()
    for i, line in enumerate(lines):
        if "Item$Properties.villagerFood:(I)" in line:
            value = operand(lines[i - 1])
            field = next(
                re.search(r"putstatic .*// Field ([A-Z_]+):", lines[j])
                for j in range(i, i + 40)
                if "putstatic" in lines[j]
            )
            if value is None or field is None:
                raise ValueError("unrecognized villagerFood call")
            out[field[1].lower()] = value
    if not out:
        raise ValueError("no villager food")
    return out


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-villagers-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True
            )

        classes = {}
        for cls, _ in CONSTANTS + INNER_CONSTANTS:
            if cls not in classes:
                classes[cls] = inspect(cls)
        constants = {
            f"{cls.rsplit('.', 1)[1]}.{name}": named_constant(classes[cls], cls, name)
            for cls, name in CONSTANTS + INNER_CONSTANTS
        }
        avoid, order = trader_goals(inspect("world.entity.npc.wanderingtrader.WanderingTrader"))

        with zipfile.ZipFile(path) as jar:
            professions = {}
            for p in PROFESSIONS:
                professions[p] = {str(level): trade_set(jar, f"{p}/level_{level}") for level in range(1, 6)}
            wandering = {name: trade_set(jar, f"wandering_trader/{name}") for name in order}
            used = sorted(
                {t for p in professions.values() for s in p.values() for t in s["trades"]}
                | {t for s in wandering.values() for t in s["trades"]}
            )
            schedule = read_json(jar, "data/minecraft/timeline/villager_schedule.json")
            schedules = {
                strip(track).split("/", 1)[1]: [
                    {"tick": k["ticks"], "activity": strip(k["value"])} for k in body["keyframes"]
                ]
                for track, body in schedule["tracks"].items()
            }
            rebalance = sorted(
                {
                    n.split("/villager_trade/", 1)[1].rsplit(".", 1)[0]
                    for n in jar.namelist()
                    if n.startswith("data/minecraft/datapacks/trade_rebalance/data/minecraft/villager_trade/")
                    and n.endswith(".json")
                }
            )
            facts = {
                "reference": "Java 26.3 behaviour data only; no artwork",
                "source": "official 26.3 server.jar via tools/extract_java_villager_fixtures.py",
                "server_sha1": SERVER_SHA1,
                "trade_sets": professions,
                "wandering_trader": {"order": order, "sets": wandering},
                "trades": {t: trade(jar, t) for t in used},
                "trade_rebalance_pack_trades": rebalance,
                "enchantment_tags": {t: resolve_tag(jar, "enchantment", t) for t in ENCHANTMENT_TAGS},
                "potion_tags": {"tradeable": resolve_tag(jar, "potion", "tradeable")},
                "item_tags": {
                    t: resolve_tag(jar, "item", t) for t in ["villager_picks_up", "villager_plantable_seeds"]
                },
                "schedule": {"period_ticks": schedule["period_ticks"], "tracks": schedules},
                "level_xp_thresholds": xp_thresholds(inspect("world.entity.npc.villager.VillagerData")),
                "gossip_types": gossip_types(inspect("world.entity.ai.gossip.GossipType")),
                "poi_types": poi_types(inspect("world.entity.ai.village.poi.PoiTypes")),
                "villager_hostile_distances": hostile_distances(
                    inspect("world.entity.ai.sensing.VillagerHostilesSensor")
                ),
                "wandering_trader_avoids": avoid,
                "villager_food": villager_food(inspect("world.item.Items")),
                "constants": constants,
            }
    return facts


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("server", type=Path, help="the official 26.3 server.jar (bundler)")
    parser.add_argument("output", type=Path, help="where to write villagers.json")
    args = parser.parse_args()
    facts = extract(args.server)
    args.output.write_text(json.dumps(facts, indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
