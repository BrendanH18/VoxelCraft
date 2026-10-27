#!/usr/bin/env python3
"""Normalize every Java 26.3 recipe in data/minecraft/recipe/ from a verified official server jar.

The output keeps only the facts the recipe comparison needs: shapes, ingredient
alternatives (item tags are kept by name and expanded once in `item_tags`),
result ids and counts, cooking times and experience, stonecutting counts and
smithing slots. Brewing (`recipe/brewing/`) is a separate system and is not
included. The extractor never reads VoxelCraft source.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import zipfile


SERVER_SHA1 = "33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c"
SOURCE = f"https://piston-data.mojang.com/v1/objects/{SERVER_SHA1}/server.jar"
RECIPES = "data/minecraft/recipe/"
TAGS = "data/minecraft/tags/item/"
COOKING = ("smelting", "blasting", "smoking", "campfire_cooking")
# Recipe types the normalizer understands, besides `crafting_special_*`; an
# unknown type fails rather than being skipped.
KNOWN_TYPES = {
    "crafting_shaped", "crafting_shapeless", "crafting_transmute", "crafting_dye", "crafting_imbue",
    "crafting_decorated_pot", "stonecutting", "smithing_transform", "smithing_trim", *COOKING,
}


def short(name):
    if not name.startswith(("minecraft:", "#minecraft:")):
        raise ValueError(f"non-vanilla id {name}")
    return name.replace("minecraft:", "")


def ingredient(value):
    """An item id, a `#tag`, or a list of item ids."""
    if isinstance(value, str):
        return short(value)
    if isinstance(value, list) and all(isinstance(v, str) and not v.startswith("#") for v in value):
        return [short(v) for v in value]
    raise ValueError(f"unknown ingredient form {value!r}")


def result(recipe):
    out = recipe["result"]
    if set(out) - {"id", "count", "components"}:
        raise ValueError(f"unknown result keys {sorted(out)}")
    # An empty transmute result keeps the input item (map cloning).
    fact = {"result": short(out["id"]) if "id" in out else None}
    if out.get("count", 1) != 1:  # omitted means one
        fact["count"] = out["count"]
    if "components" in out:
        fact["components"] = out["components"]
    return fact


def normalize(kind, recipe):
    if kind == "crafting_shaped":
        width = len(recipe["pattern"][0])
        if any(len(row) != width for row in recipe["pattern"]):
            raise ValueError("ragged pattern")
        return {"pattern": recipe["pattern"], "key": {k: ingredient(v) for k, v in sorted(recipe["key"].items())},
                **result(recipe)}
    if kind == "crafting_shapeless":
        return {"ingredients": [ingredient(v) for v in recipe["ingredients"]], **result(recipe)}
    if kind == "crafting_transmute":
        fact = {"input": ingredient(recipe["input"]), "material": ingredient(recipe["material"]), **result(recipe)}
        for extra in ("material_count", "add_material_count_to_result"):
            if extra in recipe:
                fact[extra] = recipe[extra]
        return fact
    if kind == "crafting_dye":
        return {"target": ingredient(recipe["target"]), "dye": ingredient(recipe["dye"]), **result(recipe)}
    if kind == "crafting_imbue":
        return {"source": ingredient(recipe["source"]), "material": ingredient(recipe["material"]), **result(recipe)}
    if kind == "crafting_decorated_pot":
        return {side: ingredient(recipe[side]) for side in ("back", "left", "right", "front")} | result(recipe)
    if kind in COOKING:
        if "cookingtime" not in recipe or "experience" not in recipe:
            raise ValueError("cooking recipe without explicit time and experience")
        return {"ingredient": ingredient(recipe["ingredient"]), **result(recipe),
                "cookingtime": recipe["cookingtime"], "experience": recipe["experience"]}
    if kind == "stonecutting":
        return {"ingredient": ingredient(recipe["ingredient"]), **result(recipe)}
    if kind == "smithing_transform":
        return {slot: ingredient(recipe[slot]) for slot in ("template", "base", "addition")} | result(recipe)
    if kind == "smithing_trim":
        return {slot: ingredient(recipe[slot]) for slot in ("template", "base", "addition")} | {
            "pattern": short(recipe["pattern"])}
    if kind.startswith("crafting_special_"):
        def slot(key, value):
            if key == "result":
                return short(value["id"]) if "id" in value else None
            if isinstance(value, dict):  # firework star shapes: shape -> ingredient
                return {k: ingredient(v) for k, v in sorted(value.items())}
            return ingredient(value)
        return {k: slot(k, v) for k, v in sorted(recipe.items()) if k != "type"}
    raise ValueError(f"unknown recipe type {kind}")


def tags_in(value):
    if isinstance(value, str):
        return [value[1:]] if value.startswith("#") else []
    if isinstance(value, list):
        return [t for v in value for t in tags_in(v)]
    if isinstance(value, dict):
        return [t for k, v in value.items() if k not in ("result", "components", "pattern") for t in tags_in(v)]
    return []


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("server jar is not the selected official Java 26.3 artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        names = sorted(n for n in jar.namelist()
                       if n.startswith(RECIPES) and n.endswith(".json") and "/" not in n[len(RECIPES):])
        recipes, counts = {}, {}
        for path in names:
            recipe = json.loads(jar.read(path))
            kind = short(recipe["type"])
            if kind not in KNOWN_TYPES and not kind.startswith("crafting_special_"):
                raise ValueError(f"{path}: unknown recipe type {kind}")
            recipes.setdefault(kind, {})[path[len(RECIPES):-5]] = normalize(kind, recipe)
            counts[kind] = counts.get(kind, 0) + 1

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
                for item in (expand(value[1:], (*visiting, tag)) if value.startswith("#") else [value]):
                    if item not in out:
                        out.append(item)
            cache[tag] = out
            return out

        # 26.3 recipes all say 200 ticks; the furnace divides by the fuel's
        # speed multiplier (`cooking/speed_default`), 2.0 in the fast-cooking
        # blocks. Smokers run `smoking` and blast furnaces `blasting` recipes
        # (SmokerBlockEntity / BlastFurnaceBlockEntity constructors).
        def provider(path):
            return json.loads(jar.read(f"data/minecraft/{path}.json"))

        speed = provider("context_float_provider/cooking/speed_default")
        fast = provider("predicate/block/fast_cooking")
        if speed != {"type": "minecraft:conditional", "condition": "minecraft:block/fast_cooking",
                     "on_false": "minecraft:cooking/normal_speed_multiplier",
                     "on_true": "minecraft:cooking/fast_speed_multiplier"} or fast["type"] != "minecraft:match_block":
            raise ValueError("unexpected cooking speed provider")
        cooking_speed = {
            "normal": provider("context_float_provider/cooking/normal_speed_multiplier"),
            "fast": provider("context_float_provider/cooking/fast_speed_multiplier"),
            "fast_blocks": sorted(short(b) for b in fast["blocks"]),
            "recipe_blocks": {"smelting": "furnace", "blasting": "blast_furnace", "smoking": "smoker"},
        }

        referenced = sorted({t for kind in recipes.values() for r in kind.values() for t in tags_in(r)})
        item_tags = {tag: expand(tag) for tag in referenced}
    return {
        "reference": "Minecraft Java Edition 26.3",
        "source": SOURCE,
        "server_sha1": SERVER_SHA1,
        "recipe_sources": [f"{RECIPES}*.json (not brewing/)", f"{TAGS}*.json (referenced tags, expanded)",
                           "data/minecraft/context_float_provider/cooking/*.json",
                           "data/minecraft/predicate/block/fast_cooking.json"],
        "counts": dict(sorted(counts.items())),
        "cooking_speed": cooking_speed,
        "item_tags": item_tags,
        "recipes": {kind: dict(sorted(entries.items())) for kind, entries in sorted(recipes.items())},
    }


def dump(fixture):
    """Two levels of indentation, then one recipe or tag per line."""
    def compact(value):
        return json.dumps(value, separators=(",", ":"))

    lines = ["{"]
    top = list(fixture.items())
    for i, (key, value) in enumerate(top):
        end = "," if i + 1 < len(top) else ""
        if key in ("item_tags", "recipes"):
            lines.append(f"  {compact(key)}: {{")
            groups = list(value.items())
            for j, (name, entries) in enumerate(groups):
                gend = "," if j + 1 < len(groups) else ""
                if key == "item_tags":
                    lines.append(f"    {compact(name)}: {compact(entries)}{gend}")
                    continue
                lines.append(f"    {compact(name)}: {{")
                rows = list(entries.items())
                for k, (rid, recipe) in enumerate(rows):
                    lines.append(f"      {compact(rid)}: {compact(recipe)}{',' if k + 1 < len(rows) else ''}")
                lines.append(f"    }}{gend}")
            lines.append(f"  }}{end}")
        else:
            lines.append(f"  {compact(key)}: {compact(value)}{end}")
    lines.append("}")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        fixture = extract(args.server)
    except (OSError, ValueError, KeyError, zipfile.BadZipFile) as error:
        parser.error(str(error))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(dump(fixture), encoding="utf-8")


if __name__ == "__main__":
    main()
