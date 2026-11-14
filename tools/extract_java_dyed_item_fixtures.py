#!/usr/bin/env python3
"""Extract the Java 26.3 facts behind dyed items (leather armor, leather horse
armor and wolf armor) and reference results of `DyedItemColor.applyDyes`.

Reads only the reference: the official server jar (item tags and recipes),
the official client jar (item models and the leather equipment asset) and
the decompiled source of the same version (constants and method bodies).
Writes normalized names and numbers, no assets.

- `tags/item/cauldron_can_remove_dye.json`: the dyeable items, which a water
  cauldron washes; `tags/item/dyes.json`: the dyes a `crafting_dye` recipe
  takes.
- `recipe/*.json` of type `minecraft:crafting_dye`: target, dye, result and
  group of each dye recipe.
- `DyeColor`: each dye's `textureDiffuseColor`.
- `DyedItemColor.LEATHER_COLOR`, and the `minecraft:dye` tint defaults of the
  dyeable items' models (`assets/minecraft/items/*.json`), with whether the
  model draws an undyed item untinted (`has_component dyed_color` false
  branch without tints).
- `assets/minecraft/equipment/leather.json`: which layers are `dyeable` and
  their `color_when_undyed`.
- `CauldronInteractions.dyedItemIteration` and
  `LayeredCauldronBlock.lowerFillLevel`: checked to have the shape the facts
  below describe (no sound, the component removed, one level lower, an
  empty cauldron from level 1, `TRY_WITH_EMPTY_HAND` for an undyed item).
- `vectors`: `applyDyes` results for generated dye lists, with and without a
  current colour, from a model of the method's integer and float arithmetic.
  The method body is matched line by line first, so the model fails rather
  than drifts if the source changes.
"""
import argparse
import hashlib
import io
import itertools
import json
import re
import struct
from pathlib import Path
import zipfile

from extract_java_item_fixtures import SERVER_SHA1
from extract_java_registry_names import CLIENT_SHA1

APPLY_DYES_BODY = [
    "int redTotal = 0;",
    "int greenTotal = 0;",
    "int blueTotal = 0;",
    "int intensityTotal = 0;",
    "int colorCount = 0;",
    "if (currentDye != null) {",
    "int red = ARGB.red(currentDye.rgb());",
    "int green = ARGB.green(currentDye.rgb());",
    "int blue = ARGB.blue(currentDye.rgb());",
    "intensityTotal += Math.max(red, Math.max(green, blue));",
    "redTotal += red;",
    "greenTotal += green;",
    "blueTotal += blue;",
    "colorCount++;",
    "}",
    "for (DyeColor dye : dyes) {",
    "int color = dye.getTextureDiffuseColor();",
    "int red = ARGB.red(color);",
    "int green = ARGB.green(color);",
    "int blue = ARGB.blue(color);",
    "intensityTotal += Math.max(red, Math.max(green, blue));",
    "redTotal += red;",
    "greenTotal += green;",
    "blueTotal += blue;",
    "colorCount++;",
    "}",
    "int red = redTotal / colorCount;",
    "int green = greenTotal / colorCount;",
    "int blue = blueTotal / colorCount;",
    "float averageIntensity = (float)intensityTotal / colorCount;",
    "float resultIntensity = Math.max(red, Math.max(green, blue));",
    "red = (int)(red * averageIntensity / resultIntensity);",
    "green = (int)(green * averageIntensity / resultIntensity);",
    "blue = (int)(blue * averageIntensity / resultIntensity);",
    "int rgb = ARGB.color(0, red, green, blue);",
    "return new DyedItemColor(rgb);",
]

DYED_ITEM_ITERATION_BODY = [
    "if (!itemInHand.has(DataComponents.DYED_COLOR)) {",
    "return InteractionResult.TRY_WITH_EMPTY_HAND;",
    "} else {",
    "if (!level.isClientSide()) {",
    "itemInHand.remove(DataComponents.DYED_COLOR);",
    "player.awardStat(Stats.CLEAN_ARMOR);",
    "LayeredCauldronBlock.lowerFillLevel(state, level, pos);",
    "}",
    "return InteractionResult.SUCCESS;",
    "}",
]

LOWER_FILL_LEVEL_BODY = [
    "int newLevel = state.getValue(LEVEL) - 1;",
    "BlockState newState = newLevel == 0 ? Blocks.CAULDRON.defaultBlockState() : state.setValue(LEVEL, newLevel);",
    "level.setBlockAndUpdate(pos, newState);",
    "level.gameEvent(GameEvent.BLOCK_CHANGE, pos, GameEvent.Context.of(newState));",
]


def method_body(text, signature):
    """The statements of the method whose declaration starts with `signature`."""
    start = text.find(signature)
    if start < 0 or text.find(signature, start + 1) >= 0:
        raise ValueError(f"{signature!r} not found once")
    open_at = text.index("{", start)
    depth, i = 0, open_at
    while True:
        if text[i] == "{":
            depth += 1
        elif text[i] == "}":
            depth -= 1
            if depth == 0:
                break
        i += 1
    return [line.strip() for line in text[open_at + 1:i].splitlines() if line.strip()]


def expect_body(text, signature, body):
    found = method_body(text, signature)
    if found != body:
        raise ValueError(f"{signature} changed:\n" + "\n".join(found))


def tag(jar, name):
    values = json.loads(jar.read(f"data/minecraft/tags/item/{name}.json"))["values"]
    if not all(isinstance(v, str) and v.startswith("minecraft:") for v in values):
        raise ValueError(f"{name}: unexpected entries")
    return [v[len("minecraft:"):] for v in values]


def strip(name):
    if not name.startswith("minecraft:"):
        raise ValueError(f"unexpected id {name}")
    return name[len("minecraft:"):]


def dye_recipes(jar):
    out = {}
    for n in sorted(jar.namelist()):
        if not (n.startswith("data/minecraft/recipe/") and n.endswith(".json")):
            continue
        r = json.loads(jar.read(n))
        if r["type"] != "minecraft:crafting_dye":
            continue
        if set(r) != {"type", "dye", "group", "result", "target"} or set(r["result"]) != {"id"}:
            raise ValueError(f"{n}: unexpected crafting_dye fields")
        out[n[len("data/minecraft/recipe/"):-len(".json")]] = {
            "target": strip(r["target"]),
            "dye": "#" + strip(r["dye"][1:]) if r["dye"].startswith("#") else strip(r["dye"]),
            "result": strip(r["result"]["id"]),
            "group": r["group"],
        }
    return out


def dye_colors(source):
    text = (source / "world/item/DyeColor.java").read_text()
    rows = re.findall(r"^\s+([A-Z_]+)\((\d+), \"(\w+)\", (\d+), MapColor", text, re.M)
    if len(rows) != 16 or [int(r[1]) for r in rows] != list(range(16)):
        raise ValueError("DyeColor constants changed shape")
    if "this.textureDiffuseColor = ARGB.opaque(textureDiffuseColor);" not in text:
        raise ValueError("DyeColor diffuse colour assignment changed")
    return [(name, int(rgb) & 0xFFFFFF) for _, _, name, rgb in rows]


def leather_color(source):
    text = (source / "world/item/component/DyedItemColor.java").read_text()
    found = re.findall(r"public static final int LEATHER_COLOR = (-?\d+);", text)
    if len(found) != 1:
        raise ValueError("LEATHER_COLOR not found once")
    return int(found[0])


def model_tints(node, out):
    """Collects (dye tint defaults, untinted dyed_color false branch)."""
    if isinstance(node, dict):
        if node.get("type") == "minecraft:dye" and "default" in node:
            out["dye_defaults"].add(node["default"])
        if node.get("type") == "minecraft:condition" and node.get("property") == "minecraft:has_component" \
                and node.get("component") == "minecraft:dyed_color" and "tints" not in node["on_false"]:
            out["undyed_untinted"] = True
        for v in node.values():
            model_tints(v, out)
    elif isinstance(node, list):
        for v in node:
            model_tints(v, out)


def hex6(rgb):
    return f"#{rgb & 0xFFFFFF:06X}"


def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


def java_int(x):
    """Java's `(int)` of a float: toward zero, NaN to 0."""
    if x != x:
        return 0
    return int(x)


def apply_dyes(current, dyes):
    """`DyedItemColor.applyDyes(currentDye, dyes)` (body matched above). The
    products and quotients of two floats are exact in double before the one
    rounding to float, so each `f32(...)` is the Java float operation."""
    colors = ([current] if current is not None else []) + list(dyes)
    channels = [[(c >> 16) & 255, (c >> 8) & 255, c & 255] for c in colors]
    totals = [sum(c[i] for c in channels) for i in range(3)]
    intensity = sum(max(c) for c in channels)
    count = len(channels)
    avg = [t // count for t in totals]
    average_intensity = f32(intensity / count)
    result_intensity = float(max(avg))
    out = []
    for v in avg:
        product = f32(v * average_intensity)
        out.append(java_int(f32(product / result_intensity)) if result_intensity != 0 else 0)
    return ((out[0] & 255) << 16) | ((out[1] & 255) << 8) | (out[2] & 255)


def vectors(colors):
    names = [n for n, _ in colors]
    rgb = dict(colors)
    cases = []

    def add(current, dyes):
        cases.append([None if current is None else hex6(current), list(dyes),
                      hex6(apply_dyes(current, [rgb[d] for d in dyes]))])

    # Prior colours: a mixed result, a single dye, black, white and an
    # arbitrary colour (a dyed item can hold any RGB through commands).
    priors = [apply_dyes(None, [rgb["red"], rgb["blue"]]), rgb["lime"], 0x000000, 0xFFFFFF, 0x123456]
    for d in names:
        add(None, [d])
        for p in priors:
            add(p, [d])
    for pair in itertools.combinations_with_replacement(names, 2):
        add(None, pair)
        add(priors[0], pair)
    for triple in itertools.combinations_with_replacement(names, 3):
        add(None, triple)
    # A full crafting table: the target and eight dyes.
    for d in names:
        add(None, [d] * 8)
    for start in range(0, 16, 2):
        add(None, [names[(start + k) % 16] for k in range(8)])
        add(priors[4], [names[(start + 3 * k) % 16] for k in range(8)])
    return cases


def extract(server, client, source):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    client_data = client.read_bytes()
    if hashlib.sha1(client_data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        dyeable = tag(jar, "cauldron_can_remove_dye")
        dyes = tag(jar, "dyes")
        recipes = dye_recipes(jar)
    models = {}
    with zipfile.ZipFile(io.BytesIO(client_data)) as jar:
        for item in dyeable:
            found = {"dye_defaults": set(), "undyed_untinted": False}
            model_tints(json.loads(jar.read(f"assets/minecraft/items/{item}.json")), found)
            models[item] = {"dye_defaults": sorted(found["dye_defaults"]),
                            "undyed_untinted": found["undyed_untinted"]}
        equipment = json.loads(jar.read("assets/minecraft/equipment/leather.json"))
    leather_layers = {}
    for kind, layers in sorted(equipment["layers"].items()):
        leather_layers[kind] = [
            {"texture": strip(l["texture"]),
             "color_when_undyed": l["dyeable"].get("color_when_undyed") if "dyeable" in l else None,
             "dyeable": "dyeable" in l}
            for l in layers
        ]
    dyed_item_color = (source / "world/item/component/DyedItemColor.java").read_text()
    expect_body(dyed_item_color, "public static DyedItemColor applyDyes(@Nullable final DyedItemColor currentDye",
                APPLY_DYES_BODY)
    cauldron = (source / "core/cauldron/CauldronInteractions.java").read_text()
    if "WATER.put(ItemTags.CAULDRON_CAN_REMOVE_DYE, CauldronInteractions::dyedItemIteration);" not in cauldron:
        raise ValueError("dyedItemIteration is no longer the water cauldron's #cauldron_can_remove_dye use")
    if "LAVA.put(ItemTags.CAULDRON_CAN_REMOVE_DYE" in cauldron or "POWDER_SNOW.put(ItemTags.CAULDRON_CAN_REMOVE_DYE" in cauldron:
        raise ValueError("dyedItemIteration registered on another cauldron")
    expect_body(cauldron, "private static InteractionResult dyedItemIteration(", DYED_ITEM_ITERATION_BODY)
    layered = (source / "world/level/block/LayeredCauldronBlock.java").read_text()
    expect_body(layered, "public static void lowerFillLevel(", LOWER_FILL_LEVEL_BODY)
    colors = dye_colors(source)
    if [n + "_dye" for n, _ in colors] != dyes:
        raise ValueError("#dyes is no longer the DyeColor order")
    leather = leather_color(source)
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "client_sha1": CLIENT_SHA1,
        "sources": [
            "data/minecraft/tags/item/cauldron_can_remove_dye.json",
            "data/minecraft/tags/item/dyes.json",
            "data/minecraft/recipe/*.json (type minecraft:crafting_dye)",
            "assets/minecraft/items/<dyeable item>.json (minecraft:dye tints)",
            "assets/minecraft/equipment/leather.json",
            "net/minecraft/world/item/DyeColor.java (textureDiffuseColor)",
            "net/minecraft/world/item/component/DyedItemColor.java (LEATHER_COLOR, applyDyes)",
            "net/minecraft/core/cauldron/CauldronInteractions.java (dyedItemIteration)",
            "net/minecraft/world/level/block/LayeredCauldronBlock.java (lowerFillLevel)",
        ],
        "dyeable": dyeable,
        "dyes": dyes,
        "dye_colors": {n: hex6(c) for n, c in colors},
        "leather_color": leather,
        "leather_color_rgb": hex6(leather),
        "item_models": models,
        "leather_equipment_layers": leather_layers,
        "recipes": recipes,
        "cauldron_wash": {
            "cauldron": "water_cauldron",
            "removes": "dyed_color",
            "levels_lowered": 1,
            "empty_below_level": 1,
            "sound": None,
            "undyed": "TRY_WITH_EMPTY_HAND",
        },
        "vectors": vectors(colors),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("server", type=Path, help="official Java 26.3 server.jar (the bundler)")
    parser.add_argument("client", type=Path, help="official Java 26.3 client.jar")
    parser.add_argument("source", type=Path, help="decompiled net/minecraft source folder of the same version")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    text = json.dumps(extract(args.server, args.client, args.source), indent=1)
    # One vector per line keeps the file readable and diffs small.
    text = re.sub(r'\[\n\s+("#[0-9A-F]{6}"|null),\n\s+\[\n((?:\s+"[a-z_]+",?\n)+)\s+\],\n\s+("#[0-9A-F]{6}")\n\s+\]',
                  lambda m: "[" + m.group(1) + ", [" + ", ".join(re.findall(r'"[a-z_]+"', m.group(2))) + "], "
                  + m.group(3) + "]", text)
    args.output.write_text(text + "\n")


if __name__ == "__main__":
    main()
