#!/usr/bin/env python3
"""Normalize Java 26.3 per-block mining properties for every registered block.

Hardness (`destroyTime`), blast resistance and `requiresCorrectToolForDrops`
come from evaluating `Blocks.<clinit>` bytecode (see `java_block_properties`);
the per-tier `incorrect_for_<tier>_tool` block tags come from data, with
nested tags expanded. The mineable, sword and shears tags are already in
`mining.json`. The extractor checks the `Player.hasCorrectToolForDrops` and
`Tool.isCorrectForDrops` forms the fixture's harvest rule relies on, and never
reads VoxelCraft source.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1
from java_block_properties import block_properties

TIERS = ("wooden", "stone", "copper", "iron", "gold", "diamond", "netherite")


def expand(jar, kind, name, stack=()):
    if name in stack:
        raise ValueError("reference tag cycle")
    values = set()
    for value in json.loads(jar.read(f"data/minecraft/tags/{kind}/{name}.json"))["values"]:
        if not isinstance(value, str):
            raise ValueError("unknown tag entry form")
        if value.startswith("#minecraft:"):
            values |= expand(jar, kind, value.removeprefix("#minecraft:"), (*stack, name))
        elif value.startswith("minecraft:"):
            values.add(value.removeprefix("minecraft:"))
        else:
            raise ValueError("unexpected namespace")
    return values


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:
        incorrect = {tier: sorted(expand(jar, "block", f"incorrect_for_{tier}_tool")) for tier in TIERS}
        loot_tables = {n.removeprefix("data/minecraft/loot_table/blocks/").removesuffix(".json")
                       for n in jar.namelist() if n.startswith("data/minecraft/loot_table/blocks/") and n.endswith(".json")}
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-block-mining-") as temporary:
        jar_path = Path(temporary) / "reference.jar"
        jar_path.write_bytes(inner)

        def method(cls, name):
            code = subprocess.check_output(["javap", "-classpath", str(jar_path), "-c", "-p", cls], text=True)
            return code.split(f" {name}(")[1].split("\n\n")[0]

        player = method("net.minecraft.world.entity.player.Player", "hasCorrectToolForDrops")
        if ("BlockState.requiresCorrectToolForDrops:()Z" not in player
                or "ItemStack.isCorrectToolForDrops:" not in player or player.count("ifeq") != 2):
            raise ValueError("Player.hasCorrectToolForDrops changed")
        tool = method("net.minecraft.world.item.component.Tool", "isCorrectForDrops")
        if "Tool$Rule.correctForDrops:" not in tool or "iconst_0\n" not in tool.split("goto")[-1]:
            raise ValueError("Tool.isCorrectForDrops changed")
        rules = method("net.minecraft.world.item.ToolMaterial", "applyToolProperties")
        if rules.find("Tool$Rule.deniesDrops:") > rules.find("Tool$Rule.minesAndDrops:") or "deniesDrops" not in rules:
            raise ValueError("tier tools no longer deny incorrect_for_<tier>_tool drops before mineable/<kind>")
        for cls, name in (("net.minecraft.world.item.ToolMaterial", "applySwordProperties"),
                          ("net.minecraft.world.item.ShearsItem", "createToolProperties")):
            special = method(cls, name)
            if (special.count("Tool$Rule.minesAndDrops:") != 1 or "Tool$Rule.deniesDrops:" in special
                    or special.find("Blocks.COBWEB:") > special.find("Tool$Rule.minesAndDrops:")):
                raise ValueError(f"{name}: only cobweb is correct for drops no longer holds")
        properties = block_properties(jar_path)
    unknown = sorted(name for names in incorrect.values() for name in names if name not in properties)
    if unknown or not loot_tables <= properties.keys():
        raise ValueError(f"tag or loot table names a block Blocks doesn't register: {unknown}")
    blocks = {name: {"hardness": hardness, "blast_resistance": resistance, "requires_correct_tool": requires}
              for name, (hardness, resistance, requires) in sorted(properties.items())}
    return {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1,
            "sources": ["net.minecraft.world.level.block.Blocks.<clinit> (BlockBehaviour.Properties strength, "
                        "destroyTime, explosionResistance, instabreak, requiresCorrectToolForDrops, ofFullCopy, "
                        "ofLegacyCopy; ColorCollection and WeatheringCopperCollection registrations)",
                        "net.minecraft.references.BlockItemIds and BlockIds", "Player.hasCorrectToolForDrops",
                        "Tool.isCorrectForDrops", "ToolMaterial.applyToolProperties",
                        "ToolMaterial.applySwordProperties", "ShearsItem.createToolProperties",
                        *(f"data/minecraft/tags/block/incorrect_for_{tier}_tool.json" for tier in TIERS)],
            "harvest_rule": "a block drops when it doesn't require the correct tool, or the held tool's rules "
                            "(incorrect_for_<tier>_tool denies, then mineable/<kind> allows) accept it",
            "correct_for_drops_only": {"sword": ["cobweb"], "shears": ["cobweb"]},
            "blocks": blocks, "incorrect_for_tool": incorrect}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = extract(args.server)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # One line per block keeps the 1,286 entries reviewable in a diff.
    blocks = result.pop("blocks")
    text = json.dumps(result, indent=2)
    lines = ",\n".join(f"    {json.dumps(name)}: {json.dumps(facts)}" for name, facts in blocks.items())
    text = text[:-2] + ',\n  "blocks": {\n' + lines + "\n  }\n}\n"
    json.loads(text)
    args.output.write_text(text)


if __name__ == "__main__":
    main()
