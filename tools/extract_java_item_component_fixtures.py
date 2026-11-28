#!/usr/bin/env python3
"""Extract each Java 26.3 item's default component count for advanced tooltips.

Java's advanced tooltip (F3+H) ends with "N component(s)", N being the size
of the stack's component map: the item's default components plus whatever
the stack adds. The data generator's reports list each item's defaults:

    java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar \\
        --reports --output out
    python3 tools/extract_java_item_component_fixtures.py out/reports \\
        --output tests/fixtures/java/26.3/item_components.json

For each item the fixture keeps the count and which of the components a
VoxelCraft stack can carry are among its defaults (setting one of those
doesn't change the count; adding any other does). Names and numbers only.
"""

import argparse
import hashlib
import json
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1

# The components a VoxelCraft stack can set (`inventory::Stack` and its
# payloads), as Java names them.
TRACKED = [
    "banner_patterns",
    "base_color",
    "bundle_contents",
    "charged_projectiles",
    "container",
    "custom_name",
    "damage",
    "enchantments",
    "firework_explosion",
    "fireworks",
    "lodestone_tracker",
    "map_id",
    "repair_cost",
    "stored_enchantments",
    "trim",
    "writable_book_content",
    "written_book_content",
]


def extract(reports, server_sha1):
    root = Path(reports) / "minecraft" / "components" / "item"
    items = {}
    for path in sorted(root.glob("*.json")):
        components = json.loads(path.read_text())["components"]
        names = [name.split(":", 1)[-1] for name in components]
        # [default component count, the tracked components among them]
        items[path.stem] = [len(names), sorted(name for name in names if name in TRACKED)]
    return {
        "reference": "Minecraft Java Edition 26.3",
        "source": "data generator reports (minecraft/components/item)",
        "server_sha1": server_sha1,
        "tracked": TRACKED,
        "items": items,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("reports", help="the data generator's reports directory")
    parser.add_argument("--server", help="the server jar the reports came from (checked)")
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    if args.server:
        digest = hashlib.sha1(Path(args.server).read_bytes()).hexdigest()
        if digest != SERVER_SHA1:
            raise SystemExit("server jar is not the selected official Java 26.3 artifact")
    fixture = extract(args.reports, SERVER_SHA1)
    items = fixture.pop("items")
    head = json.dumps(fixture, indent=1, sort_keys=True)[:-2]
    lines = ",\n".join("  {}: {}".format(json.dumps(k), json.dumps(v, separators=(",", ":"))) for k, v in items.items())
    Path(args.output).write_text(head + ',\n "items": {\n' + lines + "\n }\n}\n")


if __name__ == "__main__":
    main()
