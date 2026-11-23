#!/usr/bin/env python3
"""Extract v0.17 Track C2 dispenser and hopper facts from the Java 26.3 jar.

Compiles `JavaDispenseVectors.java` against the verified official server jar
and its bundled libraries and runs it after `Bootstrap.bootStrap()`: it
reports every item whose dispense behaviour `DispenserBlock.getDispenseMethod`
resolves to something other than the plain drop (the `DispenseItemBehavior`
bootstrap registry first, then the equippable, sulfur-cube and spawn-egg
fallbacks), each projectile item's `DispenseConfig`, the boat widths the
boat behaviour spaces by, `Hopper.SUCK_AABB`, and every container block's
slots per face with the probe items it accepts and gives up through each
face. From the jar's data it adds the tags those behaviours test. Needs
`javac` and `java` (JDK 25) on `PATH`. Never reads VoxelCraft source;
nothing decompiled or copied is written.
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

HARNESS = Path(__file__).with_name("JavaDispenseVectors.java")
DATA = "data/minecraft/"

# Anonymous bootstrap behaviours are named after the first item they serve;
# these are the names the fixture uses for them. An unknown one fails.
ANONYMOUS = {
    "bootstrap:armor_stand": "armor_stand",
    "bootstrap:chest": "chest_on_mount",
    "bootstrap:water_bucket": "filled_bucket",
    "bootstrap:bucket": "empty_bucket",
    "bootstrap:bone_meal": "bone_meal",
    "bootstrap:tnt": "tnt",
    "bootstrap:wither_skeleton_skull": "wither_skull",
    "bootstrap:carved_pumpkin": "carved_pumpkin",
    "bootstrap:glass_bottle": "glass_bottle",
    "bootstrap:glowstone": "glowstone",
    "bootstrap:brush": "brush",
    "bootstrap:honeycomb": "honeycomb",
    "bootstrap:potion": "water_potion",
}
NAMED = {
    "ProjectileDispenseBehavior": "projectile",
    "BoatDispenseItemBehavior": "boat",
    "FlintAndSteelDispenseItemBehavior": "flint_and_steel",
    "ShulkerBoxDispenseBehavior": "shulker_box",
    "ShearsDispenseItemBehavior": "shears",
    "MinecartDispenseItemBehavior": "minecart",
    "EquipmentDispenseItemBehavior": "equipment",
    "SulfurCubeBlockDispenseItemBehavior": "sulfur_cube_block",
    "SpawnEggItemBehavior": "spawn_egg",
}
ITEM_TAGS = ("sulfur_cube_swallowable", "furnace_fuel_bottom_takeable", "bookshelf_books")
BLOCK_TAGS = ("does_not_block_hoppers", "convertible_to_mud", "beehives", "rails")
ENTITY_TAGS = ("can_equip_saddle", "can_equip_harness", "can_wear_horse_armor", "can_wear_nautilus_armor")
# `BrewingStandBlockEntity.canPlaceItem` asks the level's recipes for slots
# 0-3, so the oracle's level-less stand accepts nothing there.
NOTES = {
    "BrewingStandBlockEntity": "slots 0-3 accept nothing here because canPlaceItem reads the level's recipe "
    "access (BREWING_REAGENTS for slot 3; potion, splash, lingering or glass bottle into an empty slot 0-2)",
}


def expand(jar, kind, name, stack=()):
    if name in stack:
        raise ValueError(f"{kind} tag cycle at {name}")
    out = set()
    for value in json.loads(jar.read(f"{DATA}tags/{kind}/{name}.json"))["values"]:
        if isinstance(value, dict):
            value = value["id"]
        if value.startswith("#minecraft:"):
            out |= set(expand(jar, kind, value.removeprefix("#minecraft:"), (*stack, name)))
        else:
            out.add(value.removeprefix("minecraft:"))
    return sorted(out)


def run_harness(bundle, inner):
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-dispense-") as temporary:
        root = Path(temporary)
        server = root / "server.jar"
        server.write_bytes(inner)
        libraries = []
        for name in bundle.namelist():
            if name.startswith("META-INF/libraries/") and name.endswith(".jar"):
                target = root / "libraries" / Path(name).name
                target.parent.mkdir(exist_ok=True)
                target.write_bytes(bundle.read(name))
                libraries.append(str(target))
        classpath = ":".join([str(server), *libraries])
        classes = root / "classes"
        classes.mkdir()
        subprocess.run(
            ["javac", "-nowarn", "-Xlint:-deprecation", "-cp", classpath, "-d", str(classes), str(HARNESS)], check=True
        )
        result = subprocess.run(
            ["java", "-cp", f"{classes}:{classpath}", "JavaDispenseVectors"],
            capture_output=True,
            text=True,
            cwd=temporary,
        )
        if result.returncode != 0:
            raise RuntimeError("JavaDispenseVectors failed:\n" + result.stdout[-2000:] + result.stderr[-4000:])
    # The server's logger prefixes standard output with a timestamp; only the
    # oracle's JSON lines matter.
    lines = [re.sub(r"^\[[^]]*\] \[[^]]*\]: \[STDOUT\]: ", "", line) for line in result.stdout.splitlines()]
    return [json.loads(line) for line in lines if line.startswith('{"type"')]


def behaviour_name(raw):
    if raw in NAMED:
        return NAMED[raw]
    if raw in ANONYMOUS:
        return ANONYMOUS[raw]
    raise ValueError(f"unknown dispense behaviour {raw}")


def parse(lines):
    items, projectiles, boats, containers = [], {}, {}, []
    summary = hopper = None
    for line in lines:
        kind = line.pop("type")
        if kind == "item":
            line["behaviour"] = behaviour_name(line["behaviour"])
            items.append(line)
        elif kind == "projectile":
            item = line.pop("item")
            projectiles[item] = line
        elif kind == "boat":
            boats[line["entity"]] = line["width"]
        elif kind == "container":
            containers.append(line)
        elif kind == "summary":
            summary = line
        elif kind == "hopper":
            hopper = line
    if summary is None or hopper is None or len(items) < 100 or len(containers) < 20:
        raise ValueError("unexpected harness output")
    return items, projectiles, boats, containers, summary, hopper


def compress_faces(faces):
    """Runs of consecutive slots with the same probe results, per face; one
    `all` entry when every face agrees. Slot order is kept (it is the order
    hoppers try them in)."""
    out = {}
    for face, slots in faces.items():
        runs = []
        for entry in slots:
            if runs and runs[-1]["place"] == entry["place"] and runs[-1]["take"] == entry["take"]:
                runs[-1]["slots"].append(entry["slot"])
            else:
                runs.append({"slots": [entry["slot"]], "place": entry["place"], "take": entry["take"]})
        out[face] = runs
    if all(runs == out["down"] for runs in out.values()):
        return {"all": out["down"]}
    return out


def group_containers(rows):
    """One entry per distinct container rule set, listing the blocks that share it."""
    groups = {}
    for row in rows:
        row["faces"] = compress_faces(row["faces"])
        key = json.dumps({k: row[k] for k in ("class", "size", "worldly", "max_stack", "faces")}, sort_keys=True)
        entry = groups.setdefault(key, {k: row[k] for k in ("class", "size", "worldly", "max_stack", "faces")})
        if row.get("needs_level"):
            entry["needs_level"] = True
        if row["class"] in NOTES:
            entry["note"] = NOTES[row["class"]]
        label = row["block"] + (f"[{row['state']}]" if row["state"] else "")
        entry.setdefault("blocks", []).append(label)
    return sorted(groups.values(), key=lambda g: g["blocks"][0])


def extract(server_path):
    data = server_path.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundle:
        inner = bundle.read("META-INF/versions/26.3/server-26.3.jar")
        items, projectiles, boats, containers, summary, hopper = parse(run_harness(bundle, inner))
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:
        item_tags = {name: expand(jar, "item", name) for name in ITEM_TAGS}
        block_tags = {name: expand(jar, "block", name) for name in BLOCK_TAGS}
        entity_tags = {name: expand(jar, "entity_type", name) for name in ENTITY_TAGS}
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "tools/JavaDispenseVectors.java run against the jar after Bootstrap.bootStrap(): "
            "DispenserBlock.DISPENSER_REGISTRY (filled by DispenseItemBehavior.bootStrap) and "
            "DispenserBlock.getDefaultDispenseMethod for every item in BuiltInRegistries.ITEM",
            "ProjectileItem.createDispenseConfig for each ProjectileDispenseBehavior item, evaluated for every facing",
            "EntityType.getWidth for the boats BoatDispenseItemBehavior places",
            "Hopper.SUCK_AABB",
            "every block whose block entity is a Container (or a WorldlyContainerHolder): "
            "WorldlyContainer.getSlotsForFace, Container.canPlaceItem with canPlaceItemThroughFace, "
            "Container.canTakeItem (into an empty five-slot container) with canTakeItemThroughFace",
            "data/minecraft/tags/item/{" + ",".join(ITEM_TAGS) + "}.json",
            "data/minecraft/tags/block/{" + ",".join(BLOCK_TAGS) + "}.json",
            "data/minecraft/tags/entity_type/{" + ",".join(ENTITY_TAGS) + "}.json",
        ],
        "summary": {"plain_items": summary["plain_items"], "registry_size": summary["registry_size"]},
        "items": sorted(items, key=lambda i: i["item"]),
        "projectiles": projectiles,
        "boat_widths": boats,
        "hopper_suck_aabb": hopper["suck_aabb"],
        "containers": group_containers(containers),
        "item_tags": item_tags,
        "block_tags": block_tags,
        "entity_type_tags": entity_tags,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
