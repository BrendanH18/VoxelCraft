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


def extract(client):
    data = client.read_bytes()
    if hashlib.sha1(data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    out = {}
    with zipfile.ZipFile(client) as jar:
        prefix = "assets/minecraft/items/"
        for path in sorted(n for n in jar.namelist() if n.startswith(prefix) and n.endswith(".json")):
            item = path[len(prefix) : -len(".json")]
            node = leaf(json.loads(jar.read(path))["model"])
            kind = strip(node.get("type", "model"))
            if kind == "special":
                special = strip(node["model"]["type"])
                out[item] = {"kind": "special", "special": special, "base": strip(node.get("base", ""))}
            elif kind == "model":
                TEXTURES.clear()
                k, chain = kind_of(jar, node["model"])
                out[item] = {"kind": k, "model": chain[0]}
                if k == "flat" and "layer0" in TEXTURES:
                    out[item]["layer0"] = TEXTURES["layer0"]
            else:
                out[item] = {"kind": kind}
    return {
        "reference": "Minecraft Java Edition 26.3",
        "client_sha1": CLIENT_SHA1,
        "source": "assets/minecraft/items/*.json and the models/ parent chains of their GUI leaf",
        "items": out,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("client", type=Path, help="official Java 26.3 client.jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.client), indent=1, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
