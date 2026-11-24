#!/usr/bin/env python3
"""Extract Java 26.3's title panorama, world icon and Panorama Scroll Speed
facts (v0.18 Track 7).

Reads the verified client jar with `javap`: `Panorama` (the spin per tick and
its option), `CubeMap` (field of view, near and far planes), `GuiRenderer`
(the tilt it draws the cube at), `CubeMapTexture` (six faces, blurred
sampling), `LogoRenderer` (the logo box), `Options` (the option's key,
default, slider and label), `AccessibilityOptionsScreen` (the option order),
`GameRenderer` (the world icon's capture rule and size), `FaviconTexture`,
`WorldSelectionList$WorldListEntry` (the icon's list size and hover wash) and
`EditWorldScreen` (its button order and Reset Icon's rule), plus the English
strings from `en_us.json`. Nothing decompiled or copied is written; only
numbers, keys and labels.

Usage: extract_java_title_fixtures.py client.jar --output
tests/fixtures/java/26.3/title.json (`javap` must be on PATH).
"""

import argparse
import hashlib
import json
import re
import zipfile
from pathlib import Path

from extract_java_minecart_fixtures import expect, javap, literals, method
from extract_java_registry_names import CLIENT_SHA1

GUI = "net.minecraft.client.gui."
RENDERER = "net.minecraft.client.renderer."


def ints(block):
    return [v for v in literals(block) if isinstance(v, int)]


def strings(block):
    return re.findall(r"// String (\S+)", block)


def extract(client):
    data = client.read_bytes()
    if hashlib.sha1(data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned Java 26.3 client")
    with zipfile.ZipFile(client) as jar:
        lang = json.loads(jar.read("assets/minecraft/lang/en_us.json"))

    panorama = javap(client, RENDERER + "Panorama")
    extract_state = method(panorama, "void extractRenderState(")
    expect("Panorama reads the speed option", "OptionsRenderState.panoramaSpeed" in extract_state, True)
    expect("Panorama wraps its spin", "Mth.wrapDegrees" in extract_state, True)
    expect("Panorama spins in real time", "getRealtimeDeltaTicks" in extract_state, True)
    [spin_per_tick] = [v for v in literals(extract_state) if isinstance(v, float) and v not in (0.0,)]
    overlay = re.search(r"// String (textures/gui/title/background/panorama_overlay\.png)", panorama)[1]

    cube = javap(client, RENDERER + "CubeMap")
    render = method(cube, "void render(float, float)")
    near, far, fov = [v for v in literals(render) if isinstance(v, float)][:3]
    expect("CubeMap turns about X then Y", ("rotateX" in render, "rotateY" in render), (True, True))

    gui = javap(client, GUI + "render.GuiRenderer")
    calls = [b for b in gui.split("\n\n") if "CubeMap.render:(FF)V" in b]
    expect("one panorama draw", len(calls), 1)
    before = calls[0][: calls[0].index("CubeMap.render:(FF)V")]
    pitch = float(re.findall(r"// float (\S+)f", before)[-1])

    texture = javap(client, RENDERER + "texture.CubeMapTexture")
    load = method(texture, "loadContents(")
    expect("six faces", "bipush        6" in load, True)
    expect("blurred, mean mipmaps", "MipmapStrategy.MEAN" in load, True)

    logo = javap(client, GUI + "components.LogoRenderer")
    logo_box = {}
    for name in ["LOGO_WIDTH", "LOGO_HEIGHT", "DEFAULT_HEIGHT_OFFSET"]:
        logo_box[name.lower()] = int(re.search(rf"int {name}( = (\d+))?;", logo)[2] or 0)

    options = javap(client, "net.minecraft.client.Options")
    ctor = method(options, "public net.minecraft.client.Options(")
    block = ctor[ctor.index("options.accessibility.panorama_speed") :]
    block = block[: block.index("putfield")]
    expect("a unit slider", "OptionInstance$UnitDouble.INSTANCE" in block, True)
    expect("defaults to 1.0", "dconst_1" in block, True)
    expect("saved as panoramaScrollSpeed", "String panoramaScrollSpeed" in options, True)

    screen = javap(client, GUI + "screens.options.AccessibilityOptionsScreen")
    order_block = method(screen, "OptionInstance<?>[] options(")
    order = re.findall(r"Method net/minecraft/client/Options\.(\w+):\(\)", order_block)

    game = javap(client, RENDERER + "GameRenderer")
    attempt = method(game, "void tryTakeScreenshotIfNeeded()")
    expect("attempts once a second", "long 1000l" in attempt, True)
    expect("only for a local server", "isLocalServer" in attempt, True)
    take = method(game, "void takeAutoScreenshot(")
    expect("waits for every section", "hasRenderedAllSections" in take, True)
    [min_sections] = ints(take)
    resize = method(game, "lambda$takeAutoScreenshot$1(")
    icon_size = [v for v in ints(resize) if v > 2]
    expect("a square icon", len(set(icon_size)), 1)
    expect("the centred square", "resizeSubRectTo" in resize, True)
    once = method(game, "lambda$tryTakeScreenshotIfNeeded$0(")
    expect("stops once the file exists", "Files.isRegularFile" in once, True)

    favicon = javap(client, GUI + "screens.FaviconTexture")
    upload = method(favicon, " upload(")
    expect("refuses other sizes", sorted(set(ints(upload))), [icon_size[0]])

    entry = javap(client, GUI + "screens.worldselection.WorldSelectionList$WorldListEntry")
    validate = method(entry, "void validateIconFile()")
    expect("refuses symbolic links", "isSymbolicLink" in validate, True)
    draw = [b for b in entry.split("\n\n") if "FaviconTexture.textureLocation" in b and "blit" in b]
    expect("one icon draw", len(draw), 1)
    list_size = max(set(ints(draw[0])), key=ints(draw[0]).count)
    hover = re.search(r"// int (-?\d+)\n.*GuiGraphicsExtractor\.fill", draw[0])
    hover_fill = int(hover[1]) & 0xFFFFFFFF

    edit = javap(client, GUI + "screens.worldselection.EditWorldScreen")
    edit_ctor = method(edit, "private net.minecraft.client.gui.screens.worldselection.EditWorldScreen(")
    buttons = re.findall(r"getstatic\s+#\d+\s+// Field (\w+_BUTTON):", edit_ctor)
    statics = method(edit, "static {}")
    keys = dict(re.findall(r"// String (selectWorld\.edit\.\w+)\n.*\n.*putstatic\s+#\d+\s+// Field (\w+):", statics))
    keys = {field: key for key, field in keys.items()}
    reset = [b for b in edit.split("\n\n") if "deleteQuietly" in b or "getIconFile" in b and "active" in b]
    expect("Reset Icon deletes quietly and greys out", any("deleteQuietly" in b for b in reset), True)
    expect("Reset Icon needs a regular file", "Files.isRegularFile" in edit, True)

    english = {
        k: lang[k]
        for k in [
            "options.accessibility.panorama_speed",
            "options.darkMojangStudiosBackgroundColor",
            "options.darkMojangStudiosBackgroundColor.tooltip",
            "selectWorld.edit.resetIcon",
            "selectWorld.edit.title",
            "selectWorld.title",
        ]
    }
    return {
        "source": "Java 26.3 client jar (pinned SHA-1); numbers, keys and labels only",
        "client_sha1": CLIENT_SHA1,
        "panorama": {
            "fov_degrees": fov,
            "near": near,
            "far": far,
            "pitch_degrees": pitch,
            "spin_per_tick_degrees": spin_per_tick,
            "spin_uses_real_time_ticks": True,
            "faces": 6,
            "sampling": "blurred (linear) with mean mipmaps",
            "overlay_texture": overlay,
        },
        "logo": logo_box,
        "panorama_speed_option": {
            "key": "panoramaScrollSpeed",
            "label": lang["options.accessibility.panorama_speed"],
            "slider": "unit (0..1)",
            "default": 1.0,
            "value_label": "percent",
        },
        "accessibility_order": order,
        "world_icon": {
            "file": "icon.png",
            "size": icon_size[0],
            "list_size": list_size,
            "hover_fill": "0x{:08X}".format(hover_fill),
            "attempt_interval_ms": 1000,
            "min_rendered_sections_exclusive": min_sections,
            "requires_all_sections_rendered": True,
            "local_server_only": True,
            "crop": "centred square",
            "taken_after_level_before_gui": True,
            "refuses_symbolic_links": True,
        },
        # The column under the name field, top down (Save and Cancel sit below it).
        "edit_world_column": [keys.get(b, b) for b in buttons if b != "SAVE_BUTTON"],
        "english": english,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("client", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    opts = parser.parse_args()
    out = extract(opts.client)
    opts.output.write_text(json.dumps(out, indent=1, sort_keys=True) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
