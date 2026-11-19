#!/usr/bin/env python3
"""Extract the jigsaw catalogue facts of villages, trial chambers and ancient cities.

Reads the pinned official Java 26.3 server artifact and writes, for the
`village_*`, `trial_chambers` and `ancient_city` structures (FE-02, v0.15
Track G):

- each structure's jigsaw settings (start pool, size, distance, heightmap
  projection, terrain adaptation, start height, pool aliases, spawn
  override categories);
- every template pool under `worldgen/template_pool/{village,trial_chambers,
  ancient_city}`: fallback and elements (kind, location or feature, weight,
  projection, processor list);
- for every template a pool names, the facts the catalogue contract keeps
  exact: its size, its jigsaw connectors (orientation, name, target, pool),
  its residents (entity ids with villager data, cat variants, baby ages),
  and its fixed blocks of interest (bells, bed heads, job sites, loot tables,
  trial spawner and vault configurations, decorated pots, cobwebs);
- the rule processor lists those pools use.

A pool may name a template the jar lacks (`ancient_city/walls/no_corners`
names `intact_horizontal_wall_stairs_5`); it is recorded as missing.

Block layouts are not copied: the templates' geometry stays Mojang's, and
VoxelCraft's templates are original. Only counts and the connector graph
are recorded.
"""

import argparse
import gzip
import hashlib
import io
import json
import struct
import zipfile
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

STRUCTURES = [
    "village_plains",
    "village_desert",
    "village_savanna",
    "village_snowy",
    "village_taiga",
    "trial_chambers",
    "ancient_city",
]
POOL_ROOTS = ["village/", "trial_chambers/", "ancient_city/"]
JOB_SITES = [
    "barrel",
    "blast_furnace",
    "brewing_stand",
    "cartography_table",
    "cauldron",
    "water_cauldron",
    "composter",
    "fletching_table",
    "grindstone",
    "lectern",
    "loom",
    "smithing_table",
    "smoker",
    "stonecutter",
]
COUNTED = ["bell", "cobweb", "decorated_pot", "chest", "trapped_chest", "spawner", "trial_spawner", "vault"]
PREFIX = "data/minecraft/"


def strip(name):
    if not isinstance(name, str) or not name.startswith("minecraft:"):
        raise ValueError(f"not a minecraft id: {name!r}")
    return name[len("minecraft:") :]


class Nbt:
    """A minimal big-endian NBT reader (structure files are gzipped)."""

    def __init__(self, data):
        self.f = io.BytesIO(data)

    def unpack(self, fmt):
        size = struct.calcsize(fmt)
        return struct.unpack(fmt, self.f.read(size))

    def root(self):
        tag = self.f.read(1)[0]
        if tag != 10:
            raise ValueError("NBT root is not a compound")
        (n,) = self.unpack(">H")
        self.f.read(n)
        return self.payload(10)

    def payload(self, tag):
        simple = {1: ">b", 2: ">h", 3: ">i", 4: ">q", 5: ">f", 6: ">d"}
        if tag in simple:
            return self.unpack(simple[tag])[0]
        if tag == 7:
            (n,) = self.unpack(">i")
            return self.f.read(n)
        if tag == 8:
            (n,) = self.unpack(">H")
            return self.f.read(n).decode("utf-8")
        if tag == 9:
            element = self.f.read(1)[0]
            (n,) = self.unpack(">i")
            return [self.payload(element) for _ in range(n)]
        if tag == 10:
            out = {}
            while True:
                element = self.f.read(1)[0]
                if element == 0:
                    return out
                (n,) = self.unpack(">H")
                name = self.f.read(n).decode("utf-8")
                out[name] = self.payload(element)
        if tag == 11:
            (n,) = self.unpack(">i")
            return list(self.unpack(f">{n}i"))
        if tag == 12:
            (n,) = self.unpack(">i")
            return list(self.unpack(f">{n}q"))
        raise ValueError(f"unknown NBT tag {tag}")


def element_facts(element):
    kind = strip(element["element_type"])
    if kind in ("single_pool_element", "legacy_single_pool_element"):
        processors = element["processors"]
        if isinstance(processors, dict):
            if processors.get("processors"):
                raise ValueError("inline processor lists are not expected")
            processors = None
        else:
            processors = strip(processors)
        return {
            "kind": "legacy" if kind.startswith("legacy") else "single",
            "location": strip(element["location"]),
            "projection": element["projection"],
            "processors": processors,
        }
    if kind == "empty_pool_element":
        return {"kind": "empty"}
    if kind == "feature_pool_element":
        return {"kind": "feature", "feature": strip(element["feature"]), "projection": element["projection"]}
    if kind == "list_pool_element":
        return {
            "kind": "list",
            "projection": element["projection"],
            "elements": [element_facts(e) for e in element["elements"]],
        }
    raise ValueError(f"unknown pool element {kind}")


def locations(facts):
    if facts["kind"] in ("single", "legacy"):
        yield facts["location"]
    for e in facts.get("elements", []):
        yield from locations(e)


def entity_facts(nbt):
    out = {"id": strip(nbt["id"])}
    data = nbt.get("VillagerData")
    if data:
        out["profession"] = strip(data["profession"])
        out["type"] = strip(data["type"])
    if "Age" in nbt and nbt["Age"] < 0:
        out["baby"] = True
    if out["id"] == "cat":
        out["variant"] = strip(nbt["variant"])
    if out["id"] == "horse":
        out["variant"] = nbt["Variant"]
    if nbt.get("PersistenceRequired"):
        out["persistent"] = True
    return out


def template_facts(raw):
    nbt = Nbt(gzip.decompress(raw)).root()
    palette = nbt["palette"] if "palette" in nbt else nbt["palettes"][0]
    connectors = []
    counts = {}
    loot = {}
    spawners = []
    vaults = []
    for block in nbt["blocks"]:
        state = palette[block["state"]]
        name = strip(state["id"])
        props = state.get("properties", {})
        data = block.get("nbt", {})
        if name == "jigsaw":
            connector = {
                "orientation": props["orientation"],
                "name": strip(data["name"]),
                "target": strip(data["target"]),
                "pool": strip(data["pool"]),
            }
            # The joint only matters for connectors facing up or down.
            if props["orientation"].split("_")[0] in ("up", "down"):
                connector["joint"] = data.get("joint", "rollable")
            for key in ("placement_priority", "selection_priority"):
                if data.get(key, 0):
                    connector[key] = data[key]
            connectors.append(connector)
            continue
        if name.endswith("_bed") and props.get("part") == "head":
            counts["bed"] = counts.get("bed", 0) + 1
        if name in JOB_SITES or name in COUNTED:
            counts[name] = counts.get(name, 0) + 1
        if "LootTable" in data:
            key = f"{name} {strip(data['LootTable'])}"
            loot[key] = loot.get(key, 0) + 1
        if name == "trial_spawner":
            spawners.append(
                {
                    "normal": strip(data["normal_config"]) if "normal_config" in data else None,
                    "ominous": strip(data["ominous_config"]) if "ominous_config" in data else None,
                }
            )
        if name == "vault":
            config = data.get("config", {})
            vaults.append(
                {
                    "loot_table": strip(config["loot_table"]) if "loot_table" in config else None,
                    "ominous": props.get("ominous") == "true",
                }
            )
    connectors.sort(key=json.dumps)
    entities = sorted((entity_facts(e["nbt"]) for e in nbt.get("entities", [])), key=lambda e: json.dumps(e))
    spawners.sort(key=json.dumps)
    vaults.sort(key=json.dumps)
    out = {"size": nbt["size"], "connectors": connectors, "entities": entities, "blocks": dict(sorted(counts.items()))}
    if loot:
        # "<container block> <loot table>": count.
        out["loot_tables"] = dict(sorted(loot.items()))
    if spawners:
        out["trial_spawners"] = spawners
    if vaults:
        out["vaults"] = vaults
    return out


def rule_facts(rule):
    def predicate(p):
        kind = strip(p["predicate_type"])
        out = {"type": kind}
        if "block" in p:
            out["block"] = strip(p["block"])
        if "tag" in p:
            out["tag"] = strip(p["tag"])
        if "block_state" in p:
            out["block_state"] = {"id": strip(p["block_state"]["id"]), **p["block_state"].get("properties", {})}
        if "probability" in p:
            out["probability"] = p["probability"]
        return out

    output = rule["output_state"]
    output = strip(output) if isinstance(output, str) else {"id": strip(output["id"]), **output.get("properties", {})}
    out = {
        "input": predicate(rule["input_predicate"]),
        "location": predicate(rule["location_predicate"]),
        "output": output,
    }
    if "position_predicate" in rule:
        out["position"] = rule["position_predicate"]
    return out


def processor_facts(data):
    out = []
    for p in data["processors"]:
        kind = strip(p["processor_type"])
        if kind == "rule":
            out.append({"type": "rule", "rules": [rule_facts(r) for r in p["rules"]]})
        else:
            out.append({"type": kind, **{k: v for k, v in p.items() if k != "processor_type"}})
    return out


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:

        def read(path):
            return json.loads(jar.read(PREFIX + path))

        structures = {}
        for name in STRUCTURES:
            s = read(f"worldgen/structure/{name}.json")
            facts = {
                "start_pool": strip(s["start_pool"]),
                "size": s["size"],
                "max_distance_from_center": s["max_distance_from_center"],
                "project_start_to_heightmap": s.get("project_start_to_heightmap"),
                "terrain_adaptation": s.get("terrain_adaptation", "none"),
                "start_height": s["start_height"],
                "use_expansion_hack": s.get("use_expansion_hack", False),
                "spawn_overrides": sorted(s.get("spawn_overrides", {})),
            }
            if "start_jigsaw_name" in s:
                facts["start_jigsaw_name"] = strip(s["start_jigsaw_name"])
            if "pool_aliases" in s:
                facts["pool_aliases"] = s["pool_aliases"]
            structures[name] = facts

        pools = {}
        pool_dir = PREFIX + "worldgen/template_pool/"
        for path in sorted(jar.namelist()):
            if not path.startswith(pool_dir) or not path.endswith(".json"):
                continue
            name = path[len(pool_dir) : -len(".json")]
            if not any(name.startswith(r) for r in POOL_ROOTS):
                continue
            pool = json.loads(jar.read(path))
            pools[name] = {
                "fallback": strip(pool["fallback"]),
                "elements": [{"weight": e["weight"], **element_facts(e["element"])} for e in pool["elements"]],
            }

        templates = {}
        processors = set()
        names = set(jar.namelist())
        for pool in pools.values():
            for e in pool["elements"]:
                for location in locations(e):
                    if location in templates:
                        continue
                    path = f"{PREFIX}structure/{location}.nbt"
                    if path in names:
                        templates[location] = template_facts(jar.read(path))
                    else:
                        # Java's template manager hands back an empty template:
                        # size 0, no connectors, so it never fits.
                        templates[location] = {"missing": True}
                stack = [e]
                while stack:
                    x = stack.pop()
                    if x.get("processors"):
                        processors.add(x["processors"])
                    stack.extend(x.get("elements", []))
        processor_lists = {p: processor_facts(read(f"worldgen/processor_list/{p}.json")) for p in sorted(processors)}

    return {
        "reference": {
            "version": "26.3",
            "server_sha1": SERVER_SHA1,
            "sources": [
                "data/minecraft/worldgen/structure/{village_*,trial_chambers,ancient_city}.json",
                "data/minecraft/worldgen/template_pool/{village,trial_chambers,ancient_city}/**",
                "data/minecraft/structure/** (sizes, jigsaw blocks, entities, counted blocks only)",
                "data/minecraft/worldgen/processor_list/*.json (lists the pools name)",
            ],
        },
        "structures": structures,
        "pools": pools,
        "templates": dict(sorted(templates.items())),
        "processor_lists": processor_lists,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
