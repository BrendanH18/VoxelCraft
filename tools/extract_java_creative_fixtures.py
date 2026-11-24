#!/usr/bin/env python3
"""Extract Java 26.3's creative inventory: tabs, their item order, the screen
layout and the spawn eggs (v0.18 Track 4).

Compiles `JavaCreativeTabs.java` against the verified official server jar
and its bundled libraries and runs it: it builds every creative tab with
`CreativeModeTabs.tryRebuildTabContents` (default feature flags, without and
with operator permissions) and lists each tab's display and search-tab
stacks, and every spawn egg's entity type. From the verified client jar it
reads `CreativeModeInventoryScreen`, its `ItemPickerMenu`, `InventoryScreen`,
`HotbarManager` and `Options` with `javap` (the screen size, grid, tab,
scroller, search box and preview positions, the saved-hotbar keys) and the
English titles. Each spawn egg's two colours are the means of the two
colour clusters of its 26.3 item texture's interior pixels: numbers, not
art (VoxelCraft draws its own egg).

Writes `creative_tabs.json`, `creative_layout.json` and `spawn_eggs.json`
into the output directory. Needs `javac`, `java` and `javap` (JDK 25) on
`PATH`. Never reads VoxelCraft source; nothing decompiled or copied is
written.

Usage: extract_java_creative_fixtures.py server.jar client.jar --output-dir
tests/fixtures/java/26.3
"""

import argparse
import hashlib
import io
import json
import re
import struct
import subprocess
import tempfile
import zipfile
import zlib
from pathlib import Path

from extract_java_item_fixtures import SERVER_SHA1
from extract_java_minecart_fixtures import constant, expect, javap, literals, method
from extract_java_registry_names import CLIENT_SHA1

HARNESS = Path(__file__).with_name("JavaCreativeTabs.java")
SCREEN = "net.minecraft.client.gui.screens.inventory."
# USB HID usage ids, which 26.3's `KeyMapping` defaults use for letters.
HID_LETTERS = {4 + i: chr(ord("A") + i) for i in range(26)}


def run_harness(bundle, inner):
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-creative-") as temporary:
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
            ["java", "-cp", f"{classes}:{classpath}", "JavaCreativeTabs"],
            capture_output=True,
            text=True,
            cwd=temporary,
        )
        if result.returncode != 0:
            raise RuntimeError("JavaCreativeTabs failed:\n" + result.stdout[-2000:] + result.stderr[-4000:])
    lines = [re.sub(r"^\[[^]]*\] \[[^]]*\]: \[STDOUT\]: ", "", line) for line in result.stdout.splitlines()]
    out = [json.loads(line) for line in lines if line.startswith('{"type"')]
    if not out or out[-1]["type"] != "done":
        raise ValueError("harness output ended early")
    return out


def strip(value):
    """Drops the `minecraft:` namespace from every string in a JSON value."""
    if isinstance(value, str):
        return value[len("minecraft:") :] if value.startswith("minecraft:") else value
    if isinstance(value, list):
        return [strip(v) for v in value]
    if isinstance(value, dict):
        return {strip(k): strip(v) for k, v in value.items()}
    return value


def entry(stack):
    out = {"item": stack["item"]}
    if "components" in stack:
        out["components"] = strip(stack["components"])
    return out


def dedupe(stacks):
    """`ItemStackLinkedSet.createTypeAndComponentsSet`: first of each item and patch."""
    seen, out = set(), []
    for s in stacks:
        key = json.dumps(s, sort_keys=True)
        if key not in seen:
            seen.add(key)
            out.append(s)
    return out


def tabs_fixture(lines, lang):
    tabs = {}
    for line in lines:
        if line["type"] != "tab":
            continue
        tab = tabs.setdefault(
            line["id"],
            {
                "id": line["id"],
                "row": line["row"],
                "column": line["column"],
                "type": line["tab_type"],
                "aligned_right": line["aligned_right"],
                "show_title": line["show_title"],
                "can_scroll": line["can_scroll"],
                "title_key": line["title_key"],
                "title": lang[line["title_key"]],
                "icon": line["icon"],
                "background": strip(line["background"]),
            },
        )
        display = [entry(s) for s in line["display"]]
        search = [entry(s) for s in line["search"]]
        key = "operator" if line["permissions"] else "player"
        tab[key] = {"display": display}
        if search != display:
            tab[key]["search"] = search
    ordered = list(tabs.values())
    for tab in ordered:
        if tab["player"] == tab["operator"]:
            tab["items"] = tab.pop("player")
            del tab["operator"]
    # The search tab is every other tab's search stacks in registry order,
    # each item and patch once; it is checked here and not repeated.
    for tab in ordered:
        if tab["type"] != "search":
            continue
        for key in ("items",) if "items" in tab else ("player", "operator"):
            union = []
            for other in ordered:
                if other["type"] == "search":
                    continue
                source = other.get("items") or other[key]
                union.extend(source.get("search", source["display"]))
            if dedupe(union) != tab[key]["display"]:
                raise ValueError("reassess the search tab: it is not the other tabs' union")
            tab[key] = {"display": "union of the other tabs' search stacks", "count": len(tab[key]["display"])}
    return ordered


def png_rgba(data):
    """Decodes an 8-bit RGBA or RGB PNG, or a 1-8 bit palette one, into rows
    of (r, g, b, a)."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos, chunks, palette, alpha = 8, [], None, None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, depth, colour = struct.unpack(">IIBB", body[:10])
        elif kind == b"PLTE":
            palette = [tuple(body[i : i + 3]) for i in range(0, len(body), 3)]
        elif kind == b"tRNS":
            alpha = list(body)
        elif kind == b"IDAT":
            chunks.append(body)
    if colour not in (2, 3, 6) or (colour != 3 and depth != 8) or depth not in (1, 2, 4, 8):
        raise ValueError(f"unsupported PNG (depth {depth}, colour {colour})")
    channels = {2: 3, 3: 1, 6: 4}[colour]
    raw = zlib.decompress(b"".join(chunks))
    stride = (width * channels * depth + 7) // 8
    bpp = max(1, channels * depth // 8)
    rows, previous = [], bytearray(stride)
    for y in range(height):
        kind = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1 : (y + 1) * (stride + 1)])
        for i in range(stride):
            left = line[i - bpp] if i >= bpp else 0
            up = previous[i]
            corner = previous[i - bpp] if i >= bpp else 0
            if kind == 1:
                line[i] = (line[i] + left) & 255
            elif kind == 2:
                line[i] = (line[i] + up) & 255
            elif kind == 3:
                line[i] = (line[i] + (left + up) // 2) & 255
            elif kind == 4:
                p = left + up - corner
                pa, pb, pc = abs(p - left), abs(p - up), abs(p - corner)
                pred = left if pa <= pb and pa <= pc else up if pb <= pc else corner
                line[i] = (line[i] + pred) & 255
        previous = line
        row = []
        for x in range(width):
            if colour == 6:
                row.append(tuple(line[x * 4 : x * 4 + 4]))
            elif colour == 2:
                row.append((*line[x * 3 : x * 3 + 3], 255))
            else:
                bit = x * depth
                index = (line[bit // 8] >> (8 - depth - bit % 8)) & ((1 << depth) - 1)
                a = alpha[index] if alpha is not None and index < len(alpha) else 255
                row.append((*palette[index], a))
        rows.append(row)
    return rows


def egg_colours(rows):
    """The two colours of an egg texture: mean colours of a two-cluster
    split of its interior (opaque pixels with no transparent neighbour, so
    the outline is left out), larger cluster first."""
    height, width = len(rows), len(rows[0])

    def opaque(x, y):
        return 0 <= x < width and 0 <= y < height and rows[y][x][3] >= 128

    pixels = [
        rows[y][x][:3]
        for y in range(height)
        for x in range(width)
        if opaque(x, y) and all(opaque(x + dx, y + dy) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
    ]
    if len(pixels) < 4:
        raise ValueError("egg texture has too few interior pixels")
    counts = {}
    for p in pixels:
        counts[p] = counts.get(p, 0) + 1
    first = max(sorted(counts), key=lambda p: counts[p])

    def dist(a, b):
        return sum((x - y) ** 2 for x, y in zip(a, b))

    second = max(sorted(counts), key=lambda p: dist(p, first))
    centres = [first, second]
    for _ in range(20):
        groups = [[], []]
        for p in pixels:
            groups[0 if dist(p, centres[0]) <= dist(p, centres[1]) else 1].append(p)
        new = [
            tuple(round(sum(c[i] for c in g) / len(g)) for i in range(3)) if g else centres[k]
            for k, g in enumerate(groups)
        ]
        if new == centres:
            break
        centres = new
    groups.sort(key=len, reverse=True)
    out = [tuple(round(sum(c[i] for c in g) / len(g)) for i in range(3)) for g in groups]
    return ["#%02x%02x%02x" % c for c in out]


ENTITY = "net/minecraft/world/entity/"
FACTORY = re.compile(
    r"Field net/minecraft/world/entity/EntityTypeIds\.(\w+):.*\n\s+\d+: invokedynamic\s+#\d+,\s+0\s+"
    r"// InvokeDynamic #(\d+):create"
)
BOOTSTRAP = re.compile(
    r"^\s+(\d+): #\d+ REF_invokeStatic java/lang/invoke/LambdaMetafactory\.metafactory.*\n\s+Method arguments:\n"
    r"\s+#\d+ .*\n\s+#\d+ REF_newInvokeSpecial ([\w/$]+)\.\"<init>\"",
    re.M,
)


def entity_classes(server_path):
    """Entity id to the class its `EntityType.Builder.of(Class::new, ...)`
    factory constructs, from `EntityTypes`' static initializer."""
    code = subprocess.check_output(
        ["javap", "-c", "-p", "-v", "-classpath", str(server_path), "net.minecraft.world.entity.EntityTypes"], text=True
    )
    factories = {int(i): cls.replace("/", ".") for i, cls in BOOTSTRAP.findall(code)}
    return {field.lower(): factories[int(b)] for field, b in FACTORY.findall(code) if int(b) in factories}


def baby_rule(entity_class, server_path, cache):
    """How `SpawnEggItem.spawnOffspringFromSpawnEgg` treats a mob of this
    class: `breed` for an `AgeableMob` whose nearest `getBreedOffspring` is
    not just `return null`; `set_baby` for another mob whose class overrides
    `setBaby` below `Mob`; `none` otherwise (no offspring; the use passes)."""
    chain, name = [], entity_class
    while name and name not in ("net.minecraft.world.entity.Mob", "java.lang.Object"):
        if name not in cache:
            cache[name] = javap(server_path, name)
        chain.append((name, cache[name]))
        m = re.search(r"class [\w.$]+(?:<.*?>)? extends ([\w.$]+)", cache[name])
        name = m[1] if m else None
    if name != "net.minecraft.world.entity.Mob":
        raise ValueError(f"{entity_class} is not a Mob")
    names = [n for n, _ in chain]
    if "net.minecraft.world.entity.AgeableMob" in names:
        for _, code in chain:
            blocks = [
                b for b in code.split("\n\n") if " getBreedOffspring(net.minecraft.server.level.ServerLevel, " in b
            ]
            blocks = [b for b in blocks if "abstract" not in b.split("\n")[0] and "bridge" not in b]
            if blocks:
                steps = re.findall(r"^\s+\d+: (\w+)", blocks[0], re.M)
                return "none" if steps == ["aconst_null", "areturn"] else "breed"
        raise ValueError(f"{entity_class}: no getBreedOffspring")
    for n, code in chain:
        if n != "net.minecraft.world.entity.AgeableMob" and re.search(r" void setBaby\(boolean\);", code):
            return "set_baby"
    return "none"


def eggs_fixture(lines, client, lang, tabs, server_path):
    classes = entity_classes(server_path)
    cache = {}
    spawn_tab = next(t for t in tabs if t["id"] == "spawn_eggs")
    # The spawn egg tab (which also holds the spawners and the creaking
    # heart), then, in registry order, the eggs no tab lists (the Ender
    # Dragon's and Wither's, which only commands give).
    order = [s["item"] for s in spawn_tab["items"]["display"]]
    order += [line["item"] for line in lines if line["type"] == "egg" and line["item"] not in order]
    eggs = {}
    for line in lines:
        if line["type"] != "egg":
            continue
        rows = png_rgba(client.read(f"assets/minecraft/textures/item/{line['item']}.png"))
        base, spots = egg_colours(rows)
        eggs[line["item"]] = {
            "entity": line["entity"],
            "name": lang[f"item.minecraft.{line['item']}"],
            "category": line["category"],
            "allowed_in_peaceful": line["allowed_in_peaceful"],
            "max_stack": line["max_stack"],
            "in_tab": line["item"] in [s["item"] for s in spawn_tab["items"]["display"]],
            "class": classes[line["entity"]].rsplit(".", 1)[-1],
            "baby": "none"
            if line["entity"] == "ender_dragon"
            else baby_rule(classes[line["entity"]], server_path, cache),
            "base": base,
            "spots": spots,
        }
    order = [item for item in order if item in eggs]
    if sorted(order) != sorted(eggs):
        raise ValueError("reassess: the spawn egg tab and the SpawnEggItem set differ")
    return [dict(item=item, **eggs[item]) for item in order]


def layout_fixture(client_path, client, lang):
    screen = javap(client_path, SCREEN + "CreativeModeInventoryScreen")
    picker = javap(client_path, SCREEN + "CreativeModeInventoryScreen$ItemPickerMenu")
    inventory = javap(client_path, SCREEN + "InventoryScreen")
    container = javap(client_path, SCREEN + "AbstractContainerScreen")
    hotbars = javap(client_path, "net.minecraft.client.HotbarManager")
    options = javap(client_path, "net.minecraft.client.Options")

    size = expect("creative size", literals(method(screen, "CreativeModeInventoryScreen(net.")), [195, 136])
    search = expect("search box", literals(method(screen, " init()")), [82, 6, 80, 9, 50, 0, 0, -1, 0])
    bar = expect("scrollbar", literals(method(screen, "insideScrollbar(")), [175, 18, 14, 112, 1, 0])
    drag = expect("scroll drag", literals(method(screen, "mouseDragged(")), [18, 112, 7.5, 15.0, 0.0, 1.0, 1])
    background = expect(
        "creative background",
        literals(method(screen, "extractBackground(")),
        [0.0, 0.0, 256, 256, 175, 18, 112, 17, 12, 15, 73, 6, 105, 49, 20, 0.0625],
    )
    tab_x = expect("tab x", literals(method(screen, "getTabX(")), [27, 27, 27, 7, 1])
    expect("tab clicked", literals(method(screen, "checkTabClicked(")), [26, 32, 1, 0])
    hover = expect("tab hover", literals(method(screen, "checkTabHovering(")), [3, 3, 21, 27, 1, 0])
    button = expect(
        "tab button",
        literals(method(screen, "extractTabButton(")),
        [1, 0, 1, 0, 28, 4, 26, 32, 0, 26, 32, 13, 8, 16, 8, 1, -1],
    )
    label = expect("title", literals(method(screen, "extractLabels(")), [8, 6, -12566464.0, 0])
    select = expect(
        "inventory tab slots",
        literals(method(screen, "selectTab(")),
        [0, 9, 0, 9, 2, 0, 1, 0, 5, 9, 5, 2, 2, 54, 54, 6, 27, 5, -2000, -2000, 45, 35, 20, 9, 9, 9, 9, 18, 36, 112]
        + [54, 18, 0, 173, 112, 1, 0, 1, 0, 1, 0, 0.0, 0.0],
    )
    grid = expect(
        "picker grid", literals(method(picker, "ItemPickerMenu(net.")), [0, 0, 5, 0, 9, 9, 9, 18, 18, 18, 9, 112, 0.0]
    )
    expect("rows past the grid", literals(method(picker, "calculateRowCount(")), [9, 5])
    expect("scrolls past 45", literals(method(picker, "canScroll(")), [45, 1, 0])
    expect("row rounding", literals(method(picker, "getRowIndexForScroll(")), [0.5, 0])
    survival = expect(
        "survival preview",
        literals(method(inventory, "extractBackground(")),
        [0.0, 0.0, 256, 256, 26, 8, 75, 78, 30, 0.0625],
    )
    follow = expect(
        "follow mouse",
        literals(
            method(inventory, "extractEntityInInventoryFollowsMouse(net.minecraft.client.gui.GuiGraphicsExtractor, int")
        ),
        [
            2.0,
            2.0,
            40.0,
            40.0,
            3.1415927410125732,
            20.0,
            0.01745329238474369,
            180.0,
            20.0,
            20.0,
            20.0,
            0.0,
            1.0,
            0.0,
            2.0,
            0.0,
        ],
    )
    expect("centred panel", literals(method(container, " init()")), [2, 2])
    groups = constant(hotbars, "NUM_HOTBAR_GROUPS")
    if "String hotbar.nbt" not in hotbars:
        raise ValueError("reassess the saved hotbar file name")
    keys = {}
    for name in ("key.saveToolbarActivator", "key.loadToolbarActivator"):
        m = re.search(rf"String {re.escape(name)}\n\s*\d+: bipush\s+(\d+)\n.*KeyMapping\$Category\.CREATIVE", options)
        if not m:
            raise ValueError(f"reassess {name}")
        keys[name] = {"label": lang[name], "default": HID_LETTERS[int(m[1])]}
    # `AbstractContainerScreen`'s three-argument constructor: 176 x 166,
    # the size `InventoryScreen` keeps.
    default = expect(
        "container default size",
        literals(
            method(
                container,
                "AbstractContainerScreen(T, net.minecraft.world.entity.player.Inventory, "
                "net.minecraft.network.chat.Component);",
            )
        ),
        [176, 166],
    )

    def scale_rows(width, height):
        # `Window.calculateScale`: the largest scale leaving at least 320 x 240.
        top = 1
        while width // (top + 1) >= 320 and height // (top + 1) >= 240:
            top += 1
        rows = []
        for scale in range(1, top + 1):
            w, h = width // scale, height // scale
            rows.append({"scale": scale, "left": (w - size[0]) // 2, "top": (h - size[1]) // 2, "gui": [w, h]})
        return rows

    return {
        "reference": "Minecraft Java Edition 26.3",
        "client_sha1": CLIENT_SHA1,
        "sources": [
            SCREEN + "CreativeModeInventoryScreen (constructor, init, insideScrollbar, mouseDragged, "
            "extractBackground, getTabX/getTabY, checkTabClicked, checkTabHovering, extractTabButton, "
            "extractLabels, selectTab)",
            SCREEN + "CreativeModeInventoryScreen$ItemPickerMenu",
            SCREEN + "InventoryScreen (extractBackground, extractEntityInInventoryFollowsMouse)",
            SCREEN + "AbstractContainerScreen (init: leftPos = (width - imageWidth) / 2)",
            "net.minecraft.client.HotbarManager, net.minecraft.client.Options",
            "assets/minecraft/lang/en_us.json",
        ],
        "creative": {
            "size": size,
            "grid": {
                "columns": constant(screen, "NUM_COLS"),
                "rows": constant(screen, "NUM_ROWS"),
                "origin": [grid[5], grid[8]],
                "pitch": grid[7],
                "hotbar": [grid[10], grid[11]],
            },
            "title": {"at": label[:2], "colour": "#%06x" % (int(label[2]) & 0xFFFFFF)},
            "search_box": {"at": search[:2], "size": search[2:4], "max_length": search[4]},
            "scrollbar": {
                "at": bar[:2],
                "size": [bar[2], bar[3]],
                "thumb": [constant(screen, "SCROLLER_WIDTH"), constant(screen, "SCROLLER_HEIGHT")],
                "travel": bar[3] - background[7],
                "drag_centre": drag[2],
            },
            "tabs": {
                "size": [constant(screen, "TAB_WIDTH"), constant(screen, "TAB_HEIGHT")],
                "spacing": tab_x[0],
                "right_columns": tab_x[3],
                "right_offset": tab_x[4],
                "top_y": -constant(screen, "TAB_HEIGHT"),
                "bottom_y": size[1],
                "drawn_top_y": -button[4],
                "drawn_bottom_y": size[1] - button[5],
                "icon": {"centre": [button[11], button[13]], "top_nudge": button[15], "bottom_nudge": button[16]},
                "hover_box": hover[:4],
            },
            "inventory_tab": {
                "armor": {"origin": [select[13], select[15]], "column_pitch": select[14], "row_pitch": select[16]},
                "offhand": select[21:23],
                "main": {"origin": [select[23], select[30]], "pitch": select[27]},
                "hotbar_y": select[29],
                "destroy_slot": select[33:35],
                "preview": {"box": background[10:14], "size": background[14], "offset_y": background[15]},
            },
            "scroll": {
                "rows_visible": 5,
                "rows_scrolled": "ceil(items / 9) - 5",
                "row_rounding": 0.5,
                "scrolls_past": 45,
            },
        },
        "survival": {
            "size": default,
            "preview": {"box": survival[4:8], "size": survival[8], "offset_y": survival[9]},
        },
        "follow_mouse": {
            "divisor": follow[2],
            "degrees_per_radian_step": follow[5],
            "body_yaw": "atan((centre_x - mouse_x) / 40) * 20 degrees",
            "head_yaw": "atan((centre_x - mouse_x) / 40) * 20 degrees, relative to the body",
            "head_pitch": "-atan((centre_y - mouse_y) / 40) * 20 degrees",
            "tilt": "atan((centre_y - mouse_y) / 40) * 20 degrees about x",
        },
        "hotbars": {
            "groups": groups,
            "slots": 9,
            "file": "hotbar.nbt",
            "keys": keys,
            "info": lang["inventory.hotbarInfo"],
            "saved": lang["inventory.hotbarSaved"],
        },
        "labels": {"destroy_item": lang["inventory.binSlot"], "search_hint": lang["gui.recipebook.search_hint"]},
        "gui_scales": {"1920x1080": scale_rows(1920, 1080), "1280x720": scale_rows(1280, 720)},
    }


def extract(server_path, client_path):
    data = server_path.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    client_data = client_path.read_bytes()
    if hashlib.sha1(client_data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundle:
        inner = bundle.read("META-INF/versions/26.3/server-26.3.jar")
        lines = run_harness(bundle, inner)
    with tempfile.TemporaryDirectory() as directory, zipfile.ZipFile(io.BytesIO(client_data)) as client:
        inner_path = Path(directory) / "server.jar"
        inner_path.write_bytes(inner)
        lang = json.loads(client.read("assets/minecraft/lang/en_us.json"))
        tabs = tabs_fixture(lines, lang)
        eggs = eggs_fixture(lines, client, lang, tabs, inner_path)
        layout = layout_fixture(client_path, client, lang)
    head = {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1, "client_sha1": CLIENT_SHA1}
    creative = dict(
        head,
        sources=[
            "tools/JavaCreativeTabs.java run against the server jar after Bootstrap.bootStrap(): "
            "CreativeModeTabs.tryRebuildTabContents(FeatureFlags.DEFAULT_FLAGS, permissions, lookup) without and "
            "with operator permissions; data-driven registries listed in id order and tags in file order, as the "
            "client receives them",
            "assets/minecraft/lang/en_us.json (titles)",
        ],
        notes=[
            "Tabs are in registry order. `items` is a tab's contents when permissions change nothing; otherwise "
            "`player` and `operator` give them. `display` is what the tab shows; `search`, when present, is what it "
            "adds to the search tab instead (Java's TabVisibility).",
            "Component patches are Java's DataComponentPatch JSON with the minecraft: namespace dropped.",
        ],
        tabs=tabs,
    )
    spawn = dict(
        head,
        sources=[
            "tools/JavaCreativeTabs.java: every SpawnEggItem, SpawnEggItem.getType, EntityType.getCategory and "
            "isAllowedInPeaceful; the order is the spawn_eggs tab's, then registry order for the eggs no tab "
            "lists (`in_tab` false)",
            "`class`: the class EntityTypes' factory for the egg's type constructs (javap -v of EntityTypes). "
            "`baby`: SpawnEggItem.spawnOffspringFromSpawnEgg on a matching mob: `breed` for an AgeableMob whose "
            "nearest getBreedOffspring is not just `return null`; `set_baby` for another Mob whose class "
            "overrides setBaby below Mob; else `none` (no offspring, the use passes). The Ender Dragon is not a Mob",
            "assets/minecraft/textures/item/<egg>.png: base and spot colours are the means of a two-cluster split "
            "of the opaque interior pixels (outline excluded), larger cluster first",
            "assets/minecraft/lang/en_us.json (names)",
        ],
        eggs=eggs,
    )
    return creative, layout, spawn


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="official Java 26.3 server.jar (the bundler)")
    parser.add_argument("client", type=Path, help="official Java 26.3 client.jar")
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    creative, layout, spawn = extract(args.server, args.client)
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for name, value in (("creative_tabs", creative), ("creative_layout", layout), ("spawn_eggs", spawn)):
        (args.output_dir / f"{name}.json").write_text(json.dumps(value, indent=1) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
