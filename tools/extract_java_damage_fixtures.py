#!/usr/bin/env python3
"""Extract Java 26.3's damage types, damage type tags and the protection
enchantments' damage-source requirements, with the English death messages.

Damage types come from `data/minecraft/damage_type/*.json` (the `DamageType`
codec's optional fields filled with their defaults, `effects: hurt` and
`death_message_type: default`), tags from `data/minecraft/tags/damage_type/`
with every `#tag` reference expanded, and protection from each enchantment's
`minecraft:damage_protection` effect. Death messages are the `death.attack.*`
and `death.fell.*` keys of the client's `assets/minecraft/lang/en_us.json`.
Never reads VoxelCraft source.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import zipfile
from extract_java_item_fixtures import SERVER_SHA1
from extract_java_registry_names import CLIENT_SHA1

TYPE_KEYS = {"message_id", "scaling", "exhaustion", "effects", "death_message_type"}
SCALINGS = {"never", "when_caused_by_living_non_player", "always"}
EFFECTS = {"hurt", "thorns", "drowning", "burning", "poking", "freezing"}
DEATH_MESSAGES = {"default", "fall_variants", "intentional_game_design"}
TYPE_DIR = "data/minecraft/damage_type/"
TAG_DIR = "data/minecraft/tags/damage_type/"
ENCHANT_DIR = "data/minecraft/enchantment/"


def damage_type(name, raw):
    if not set(raw) <= TYPE_KEYS or not {"message_id", "scaling", "exhaustion"} <= set(raw):
        raise ValueError(f"unknown damage type form for {name}: {sorted(raw)}")
    entry = {"id": name, "message_id": raw["message_id"], "scaling": raw["scaling"],
             "exhaustion": raw["exhaustion"], "effects": raw.get("effects", "hurt"),
             "death_message_type": raw.get("death_message_type", "default")}
    if (entry["scaling"] not in SCALINGS or entry["effects"] not in EFFECTS
            or entry["death_message_type"] not in DEATH_MESSAGES):
        raise ValueError(f"unknown damage type enum value in {name}: {entry}")
    return entry


def expand(tags, types, name, seen=()):
    if name in seen:
        raise ValueError(f"damage type tag cycle through {name}")
    raw = tags[name]
    if set(raw) - {"values", "replace"} or raw.get("replace", False):
        raise ValueError(f"unknown damage type tag form for {name}")
    members = set()
    for value in raw["values"]:
        if not isinstance(value, str):
            raise ValueError(f"optional tag entries are not expected: {name}")
        if value.startswith("#minecraft:"):
            members |= expand(tags, types, value[len("#minecraft:"):], seen + (name,))
        elif value.startswith("minecraft:") and value[len("minecraft:"):] in types:
            members.add(value[len("minecraft:"):])
        else:
            raise ValueError(f"{name} names an unknown damage type {value}")
    return members


def protection(name, raw):
    effects = raw.get("effects", {}).get("minecraft:damage_protection")
    if effects is None:
        return None
    if len(effects) != 1:
        raise ValueError(f"{name}: expected one damage_protection effect")
    effect, requirements = effects[0]["effect"], effects[0]["requirements"]
    value = effect["value"]
    if effect["type"] != "minecraft:add" or value["type"] != "minecraft:linear":
        raise ValueError(f"{name}: unknown damage_protection effect {effect}")
    if requirements["type"] != "minecraft:damage_source_properties" or set(requirements["predicate"]) != {"tags"}:
        raise ValueError(f"{name}: unknown damage_protection requirement {requirements}")
    required, excluded = [], []
    for test in requirements["predicate"]["tags"]:
        if not test["id"].startswith("#minecraft:"):
            raise ValueError(f"{name}: requirement is not a tag: {test}")
        (required if test["expected"] else excluded).append(test["id"][len("#minecraft:"):])
    return {"enchantment": name, "base": value["base"], "per_level_above_first": value["per_level_above_first"],
            "max_level": raw["max_level"], "requires": sorted(required), "excludes": sorted(excluded)}


def extract(server, client):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    client_data = client.read_bytes()
    if hashlib.sha1(client_data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def folder(prefix):
            return {n[len(prefix):-len(".json")]: json.loads(jar.read(n)) for n in jar.namelist()
                    if n.startswith(prefix) and n.endswith(".json") and "/" not in n[len(prefix):]}
        types = {name: damage_type(name, raw) for name, raw in folder(TYPE_DIR).items()}
        raw_tags = folder(TAG_DIR)
        tags = {name: sorted(expand(raw_tags, types, name)) for name in sorted(raw_tags)}
        protections = [p for name, raw in sorted(folder(ENCHANT_DIR).items())
                       if (p := protection(name, raw)) is not None]
    for p in protections:
        if not set(p["requires"] + p["excludes"]) <= set(tags):
            raise ValueError(f"{p['enchantment']} names an unknown damage type tag")
    with zipfile.ZipFile(io.BytesIO(client_data)) as jar:
        lang = json.loads(jar.read("assets/minecraft/lang/en_us.json"))
    messages = {k: v for k, v in sorted(lang.items()) if k.startswith(("death.attack.", "death.fell."))}
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1, "client_sha1": CLIENT_SHA1,
            "sources": [TYPE_DIR + "*.json (DamageType codec defaults: effects hurt, death_message_type default)",
                        TAG_DIR + "*.json (#tag references expanded)",
                        ENCHANT_DIR + "*.json (minecraft:damage_protection)",
                        "client assets/minecraft/lang/en_us.json (death.attack.*, death.fell.*)"],
            "damage_types": [types[n] for n in sorted(types)], "tags": tags,
            "protection": protections, "death_messages": messages}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="official Java 26.3 server.jar (the bundler)")
    parser.add_argument("client", type=Path, help="official Java 26.3 client.jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    fixture = extract(args.server, args.client)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(fixture, indent=1) + "\n")


if __name__ == "__main__":
    main()
