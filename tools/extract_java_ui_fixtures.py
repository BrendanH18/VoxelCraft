#!/usr/bin/env python3
"""Extract Java 26.3's death screen, quit confirmation, menu blur option,
anvil name box and screen backdrops (v0.18 Track 2).

Reads the pinned client jar with `javap` and its language file, blur post
effect and in-world menu background, and writes the numbers and labels the
game's shared UI matches: `DeathScreen` (labels, button bounds, the
20-tick delay, title scale and text rows, the red backdrop),
`ConfirmScreen`'s layout and Escape answer, `Options.menuBackgroundBlurriness`
(key, label, tooltip, range, default), `post_effect/blur.json`'s passes,
`AnvilScreen`'s name box and cost line, and
`Screen.extractTransparentBackground`'s gradient.

Usage: extract_java_ui_fixtures.py client.jar --output
tests/fixtures/java/26.3/ui_screens.json (`javap` must be on PATH).
"""

import argparse
import hashlib
import json
import re
import struct
import subprocess
import zipfile
import zlib
from pathlib import Path

from extract_java_registry_names import CLIENT_SHA1

SCREENS = "net.minecraft.client.gui.screens."
INT = re.compile(r"\d+: (?:(iconst_m?\d)|[bs]ipush\s+(-?\d+)|ldc(?:_w)?\s+#\d+\s+// int (-?\d+))")


def javap(jar, name):
    return subprocess.check_output(["javap", "-c", "-p", "-constants", "-classpath", str(jar), name], text=True)


def method(code, signature):
    """The bytecode of the one method whose declaration contains `signature`."""
    blocks = [b for b in code.split("\n\n") if signature in b.strip().split("\n")[0]]
    if len(blocks) != 1:
        raise ValueError(f"expected one method matching {signature!r}, found {len(blocks)}")
    return blocks[0]


def ints(block):
    """Integer constants in bytecode order."""
    out = []
    for m in INT.finditer(block):
        if m[1]:
            out.append(int(m[1].split("_")[1].replace("m", "-")))
        else:
            out.append(int(m[2] or m[3]))
    return out


def expect(label, got, want):
    if got != want:
        raise ValueError(f"reassess {label}: {got!r} != {want!r}")
    return got


def argb(value):
    """A Java colour int as `0xAARRGGBB`."""
    return f"0x{value & 0xFFFFFFFF:08X}"


def png_alpha(data):
    """Every pixel's alpha of an 8-bit grey-alpha or RGBA, non-interlaced PNG."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos, idat, header = 8, b"", None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind, body = data[pos + 4 : pos + 8], data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
    width, height, depth, colour, _, _, interlace = header
    channels = {4: 2, 6: 4}.get(colour)
    if depth != 8 or interlace != 0 or channels is None:
        raise ValueError("reassess the menu background's PNG format")
    raw, stride = zlib.decompress(idat), width * channels
    prev, alphas = bytearray(stride), []
    for y in range(height):
        start = y * (stride + 1)
        kind, line = raw[start], bytearray(raw[start + 1 : start + 1 + stride])
        for x in range(stride):
            a = line[x - channels] if x >= channels else 0
            b, c = prev[x], prev[x - channels] if x >= channels else 0
            p = a + b - c
            paeth = a if abs(p - a) <= abs(p - b) and abs(p - a) <= abs(p - c) else b if abs(p - b) <= abs(p - c) else c
            line[x] = (line[x] + [0, a, b, (a + b) // 2, paeth][kind]) & 255
        alphas.extend(line[channels - 1 :: channels])
        prev = line
    return alphas


def extract(client):
    data = client.read_bytes()
    if hashlib.sha1(data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    with zipfile.ZipFile(client) as jar:
        lang = json.loads(jar.read("assets/minecraft/lang/en_us.json"))
        blur = json.loads(jar.read("assets/minecraft/post_effect/blur.json"))
        inworld = png_alpha(jar.read("assets/minecraft/textures/gui/inworld_menu_background.png"))
    death = javap(client, SCREENS + "DeathScreen")
    confirm = javap(client, SCREENS + "ConfirmScreen")
    screen = javap(client, SCREENS + "Screen")
    anvil = javap(client, SCREENS + "inventory.AnvilScreen")
    options = javap(client, "net.minecraft.client.Options")
    button = javap(client, "net.minecraft.client.gui.components.Button")

    # DeathScreen.
    init = method(death, "protected void init()")
    for key in ("deathScreen.spectate", "deathScreen.respawn", "deathScreen.titleScreen"):
        if f"String {key}" not in init:
            raise ValueError(f"reassess death screen button {key}")
    bounds = expect("death buttons", ints(init), [0, 2, 100, 4, 72, 200, 20, 2, 100, 4, 96, 200, 20, 0])
    tick = method(death, "public void tick()")
    delay = expect("death delay", ints(tick), [1, 20, 1])[1]
    text = method(death, "private void visitText(")
    rows = expect("death text rows", ints(text), [2, 2, 30, 85, 100])
    if "fconst_2" not in text:
        raise ValueError("reassess the death title's scale")
    backdrop = expect("death backdrop", ints(method(death, "extractDeathBackground(")), [0, 0, 1615855616, -1602211792])
    exit_code = method(death, "private void handleExitToTitleScreen()")
    if "this.hardcore" not in exit_code and "Field hardcore:Z" not in exit_code:
        raise ValueError("reassess Hardcore's direct exit")
    if "String deathScreen.quit.confirm" not in exit_code or "ConfirmScreen.setDelay" not in exit_code:
        raise ValueError("reassess the quit confirmation")
    confirm_delay = ints(exit_code)[-1]
    expect("confirm delay", confirm_delay, 20)
    if "shouldCloseOnEsc()" not in death or "isPauseScreen()" not in death:
        raise ValueError("reassess the death screen's Escape and pause")
    for name in ("shouldCloseOnEsc()", "isPauseScreen()"):
        expect(f"death {name}", ints(method(death, name)), [0])

    # ConfirmScreen.
    # The column's spacing is set where the `layout` field is made.
    (column,) = [b for b in confirm.split("\n\n") if "LinearLayout.vertical" in b]
    spacing = ints(column)[:1]
    layout = spacing + ints(method(confirm, "protected void init()"))
    expect("ConfirmScreen layout", layout, [8, 4, 16])
    keys = method(confirm, "keyPressed(")
    if "isEscape" not in keys or "BooleanConsumer.accept" not in keys:
        raise ValueError("reassess ConfirmScreen's Escape")
    button_w = int(re.search(r"int DEFAULT_WIDTH = (\d+);", button)[1])
    button_h = int(re.search(r"int DEFAULT_HEIGHT = (\d+);", button)[1])

    # Menu Background Blur.
    ctor = options[options.index("String options.accessibility.menu_background_blurriness") :]
    ctor = ctor[: ctor.index("putfield")]
    blur_range = expect("blur range and default", ints(ctor), [0, 10, 5])
    if "OptionInstance$IntRange" not in ctor:
        raise ValueError("reassess the blur option's range")
    if "String menuBackgroundBlurriness" not in options:
        raise ValueError("reassess the blur option's key")
    default = int(re.search(r"int BLURRINESS_DEFAULT_VALUE = (\d+);", options)[1])
    expect("blur default", default, blur_range[2])
    passes = []
    for p in blur["passes"]:
        if p["fragment_shader"] != "minecraft:post/box_blur":
            raise ValueError("reassess the blur passes")
        (config,) = [u for u in p["uniforms"]["BlurConfig"] if u["name"] == "BlurDir"]
        passes.append([int(v) for v in config["value"]])
    if not ("extractBlurredBackground" in screen and "getMenuBackgroundBlurriness" in screen):
        raise ValueError("reassess Screen's blur")

    # Backdrops.
    veil = expect(
        "container veil", ints(method(screen, "extractTransparentBackground(")), [0, 0, -1072689136, -804253680]
    )
    alphas = set(inworld)
    if len(alphas) != 1:
        raise ValueError("reassess the in-world menu background")

    # AnvilScreen.
    sub = ints(method(anvil, "protected void subInit()"))
    field = expect("anvil field", sub[2:6], [62, 24, 103, 12])
    max_len = expect("anvil max length", [v for v in sub if v == 50], [50])[0]
    sprite = ints(method(anvil, "void extractBackground("))
    sprite = expect("anvil field sprite", sprite[-4:], [59, 20, 110, 16])
    error = expect("anvil error icon", ints(method(anvil, "void extractErrorIcon("))[-4:], [99, 45, 28, 21])
    labels = ints(method(anvil, "void extractLabels("))
    expect("anvil cost colours", [labels[0], labels[2]], [-8323296, -40864])
    expect("anvil too expensive", labels[1], 40)
    expect("anvil cost rows", labels[10:16], [69, 2, 67, 8, 79, 1325400064])
    title_x = expect(
        "anvil title x", ints(method(anvil, "AnvilScreen(net.minecraft.world.inventory.AnvilMenu"))[-1:], [60]
    )[0]

    return {
        "reference": "Minecraft Java Edition 26.3",
        "client_sha1": CLIENT_SHA1,
        "sources": [
            "client DeathScreen (init, tick, visitText, extractDeathBackground, handleExitToTitleScreen,"
            " shouldCloseOnEsc, isPauseScreen) and DeathScreen.TitleConfirmScreen",
            "client ConfirmScreen (init, setDelay, tick, keyPressed); Button.DEFAULT_WIDTH, DEFAULT_HEIGHT",
            "client Options.menuBackgroundBlurriness, BLURRINESS_DEFAULT_VALUE, processOptions;"
            " assets/minecraft/post_effect/blur.json",
            "client Screen (extractBackground, extractBlurredBackground, extractTransparentBackground);"
            " assets/minecraft/textures/gui/inworld_menu_background.png",
            "client AnvilScreen (constructor, subInit, extractLabels, extractBackground, extractErrorIcon)",
            "assets/minecraft/lang/en_us.json",
        ],
        "death_screen": {
            "title": lang["deathScreen.title"],
            "title_hardcore": lang["deathScreen.title.hardcore"],
            "respawn": lang["deathScreen.respawn"],
            "spectate": lang["deathScreen.spectate"],
            "title_screen": lang["deathScreen.titleScreen"],
            "score": lang["deathScreen.score.value"],
            "button_half_width": bounds[2],
            "button_quarter_rows": [bounds[4], bounds[10]],
            "button_size": [bounds[5], bounds[6]],
            "delay_ticks": delay,
            "title_scale": rows[0],
            "title_y_scaled": rows[2],
            "cause_y": rows[3],
            "score_y": rows[4],
            "background": [argb(backdrop[2]), argb(backdrop[3])],
            "closes_on_escape": False,
            "pauses": False,
        },
        "quit_confirm": {
            "title": lang["deathScreen.quit.confirm"],
            "yes": lang["deathScreen.titleScreen"],
            "no": lang["deathScreen.respawn"],
            "delay_ticks": confirm_delay,
            "spacing": layout[0],
            "button_spacing": layout[1],
            "button_padding_top": layout[2],
            "button_size": [button_w, button_h],
            "escape_answers": "no",
        },
        "menu_blur": {
            "key": "menuBackgroundBlurriness",
            "label": lang["options.accessibility.menu_background_blurriness"],
            "tooltip": lang["options.accessibility.menu_background_blurriness.tooltip"],
            "off": lang["options.off"],
            "min": blur_range[0],
            "max": blur_range[1],
            "default": blur_range[2],
            "passes": passes,
        },
        "backgrounds": {
            "container": [argb(veil[2]), argb(veil[3])],
            "in_world_menu_alpha": alphas.pop(),
        },
        "anvil": {
            "title": lang["container.repair"],
            "title_x": title_x,
            "field": field,
            "field_max_length": max_len,
            "field_sprite": sprite,
            "error_icon": error,
            "cost": lang["container.repair.cost"],
            "too_expensive": lang["container.repair.expensive"],
            "too_expensive_at": labels[1],
            "cost_colour": argb(labels[0]),
            "short_colour": argb(labels[2]),
            "cost_y": labels[10],
            "backing_rows": [labels[12], labels[14]],
            "backing_right_margin": labels[13],
            "backing": argb(labels[15]),
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("client", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.client), indent=2) + "\n")


if __name__ == "__main__":
    main()
