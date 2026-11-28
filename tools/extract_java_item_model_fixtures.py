#!/usr/bin/env python3
"""Extract how Java 26.3 draws each item from the official client.

Every item has an item model definition (`assets/minecraft/items/<id>.json`).
Its model tree (`select`, `condition`, `range_dispatch` and `composite`
nodes) is followed to the model shown in the GUI with no components set:
a `select`'s fallback (or the case for the `gui` display context), a
`condition`'s `on_false`, a `range_dispatch`'s fallback or first entry,
and a `composite`'s first model. That leaf is either a `special` renderer
(chests, beds, banners, shields, heads...) or a plain `model`, whose
parent chain ends at `builtin/generated` (a flat sprite, extruded) or in
a block model with elements (drawn in 3D).

Each item also names its display transforms (v1.0 gap track 6c), an index
into `displays`: for every context the game draws (`gui`, `ground`, `fixed`,
`firstperson_righthand`, `thirdperson_righthand`, `on_shelf`), the transform
of the nearest model in the leaf's parent chain that sets it (Java merges
display per context) as rotation (degrees), translation (sixteenths) and
scale, nine numbers, with `ItemTransform`'s defaults filled in and a context
no model sets as the identity; and the chain's `gui_light`. A special
renderer's transforms come from its `base` model.

    python3 tools/extract_java_item_model_fixtures.py client.jar \\
        --output tests/fixtures/java/26.3/item_models.json
"""

import argparse
import hashlib
import json
import zipfile
from pathlib import Path

CLIENT_SHA1 = "e877b6a07acd633fb3bb475002175cec036e7b87"


def strip(name):
    name = name.split(":", 1)[-1]
    return name


def leaf(node):
    """The model node the GUI shows for an item with no components."""
    kind = strip(node.get("type", "model"))
    if kind == "select":
        for case in node.get("cases", []):
            when = case.get("when")
            whens = when if isinstance(when, list) else [when]
            if strip(node.get("property", "")) == "display_context" and "gui" in whens:
                return leaf(case["model"])
        if "fallback" in node:
            return leaf(node["fallback"])
        return leaf(node["cases"][0]["model"])
    if kind == "condition":
        return leaf(node["on_false"])
    if kind == "range_dispatch":
        if node.get("fallback"):
            return leaf(node["fallback"])
        return leaf(node["entries"][0]["model"])
    if kind == "composite":
        return leaf(node["models"][0])
    return node


TEXTURES = {}


def kind_of(jar, model):
    """'flat' when the parent chain reaches builtin/generated, 'block' when it
    reaches a model with elements, with the chain's names."""
    chain = []
    name = strip(model)
    while name:
        chain.append(name)
        if name == "builtin/generated":
            return "flat", chain
        try:
            data = json.loads(jar.read("assets/minecraft/models/{}.json".format(name)))
        except KeyError:
            return "missing", chain
        if "elements" in data:
            return "block", chain
        if "layer0" in data.get("textures", {}) and "layer0" not in TEXTURES:
            TEXTURES["layer0"] = strip(data["textures"]["layer0"])
        name = strip(data["parent"]) if "parent" in data else None
    return "none", chain


CONTEXTS = ["gui", "ground", "fixed", "firstperson_righthand", "thirdperson_righthand", "on_shelf"]
IDENTITY = {"rotation": [0.0, 0.0, 0.0], "translation": [0.0, 0.0, 0.0], "scale": [1.0, 1.0, 1.0]}


def display_of(jar, model):
    """The display transforms and GUI light of `model`'s parent chain."""
    found = {}
    light = None
    name = strip(model)
    while name and name != "builtin/generated":
        try:
            data = json.loads(jar.read("assets/minecraft/models/{}.json".format(name)))
        except KeyError:
            break
        for context, transform in data.get("display", {}).items():
            found.setdefault(context, transform)
        if light is None and "gui_light" in data:
            light = data["gui_light"]
        name = strip(data["parent"]) if "parent" in data else None
    out = {}
    for context in CONTEXTS:
        transform = found.get(context) or {}
        out[context] = [float(v) for key in IDENTITY for v in transform.get(key, IDENTITY[key])]
    out["gui_light"] = light or "side"
    return out


DISPLAYS = []


def display_index(jar, model):
    """The index of `model`'s display in DISPLAYS, adding it when new."""
    display = display_of(jar, model)
    if display not in DISPLAYS:
        DISPLAYS.append(display)
    return DISPLAYS.index(display)


def extract(client):
    data = client.read_bytes()
    if hashlib.sha1(data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    out = {}
    DISPLAYS.clear()
    with zipfile.ZipFile(client) as jar:
        prefix = "assets/minecraft/items/"
        for path in sorted(n for n in jar.namelist() if n.startswith(prefix) and n.endswith(".json")):
            item = path[len(prefix) : -len(".json")]
            node = leaf(json.loads(jar.read(path))["model"])
            kind = strip(node.get("type", "model"))
            if kind == "special":
                special = strip(node["model"]["type"])
                out[item] = {"kind": "special", "special": special, "base": strip(node.get("base", ""))}
                out[item]["display"] = display_index(jar, node.get("base", ""))
            elif kind == "model":
                TEXTURES.clear()
                k, chain = kind_of(jar, node["model"])
                out[item] = {"kind": k, "model": chain[0]}
                if k == "flat" and "layer0" in TEXTURES:
                    out[item]["layer0"] = TEXTURES["layer0"]
                out[item]["display"] = display_index(jar, node["model"])
            else:
                out[item] = {"kind": kind}
    return {
        "reference": "Minecraft Java Edition 26.3",
        "client_sha1": CLIENT_SHA1,
        "source": "assets/minecraft/items/*.json and the models/ parent chains of their GUI leaf, with their display transforms",
        "items": out,
        "displays": DISPLAYS,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("client", type=Path, help="official Java 26.3 client.jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.client), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
