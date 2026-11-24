#!/usr/bin/env python3
"""Extract Java 26.3's default font metrics for ASCII, Latin-1 and Latin Extended-A.

Reads the pinned client jar's `assets/minecraft/font/` provider files and
measures each glyph's advance the way `BitmapProvider.Definition.load` does:
the glyph's width is its rightmost column holding a non-transparent pixel
plus one, scaled by `height / cell height`, rounded, plus one. The space
provider's advances come from `include/space.json`. `Font.lineHeight`,
`GlyphInfo.getShadowOffset` and the signs' line widths and heights are read
from the classes with `javap`.

Only numbers are written: which provider draws each character, its advance
and the provider's ascent and height. No glyph image is copied; VoxelCraft's
font (`assets/font/`) is drawn separately and only shares these widths.

Usage: extract_java_font_fixtures.py client.jar --output
tests/fixtures/java/26.3/font_metrics.json (`javap` must be on PATH).
"""

import argparse
import hashlib
import json
import struct
import subprocess
import zipfile
import zlib
from pathlib import Path

from extract_java_registry_names import CLIENT_SHA1

# ASCII, Latin-1 Supplement and Latin Extended-A, without the C0/C1 controls.
RANGES = [(0x20, 0x7E), (0xA0, 0x17F)]


def codepoints():
    return [c for lo, hi in RANGES for c in range(lo, hi + 1)]


def paeth(a, b, c):
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    return b if pb <= pc else c


def png_alpha(data):
    """Width, height and the alpha of every pixel of a non-interlaced PNG.

    Covers what the font textures use: palette images (with `tRNS`) and
    8-bit greyscale, RGB and RGBA images.
    """
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos, idat, palette_alpha = 8, b"", None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        body = data[pos + 8 : pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if interlace:
                raise ValueError("interlaced PNGs are not supported")
        elif kind == b"tRNS":
            palette_alpha = list(body)
        elif kind == b"IDAT":
            idat += body
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[colour]
    bits = depth * channels
    stride = (width * bits + 7) // 8
    bpp = max(1, bits // 8)
    raw = zlib.decompress(idat)
    rows, prev = [], bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        kind, line = raw[start], bytearray(raw[start + 1 : start + 1 + stride])
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b, c = prev[i], prev[i - bpp] if i >= bpp else 0
            line[i] = (line[i] + (0, a, b, (a + b) // 2, paeth(a, b, c))[kind]) & 0xFF
        rows.append(line)
        prev = line

    def sample(line, x):
        if bits >= 8:
            return line[x * bpp : (x + 1) * bpp]
        per = 8 // depth
        byte = line[x // per]
        shift = 8 - depth * (x % per + 1)
        return [(byte >> shift) & ((1 << depth) - 1)]

    alpha = []
    for line in rows:
        out = []
        for x in range(width):
            px = sample(line, x)
            if colour == 3:
                index = px[0]
                out.append(palette_alpha[index] if palette_alpha and index < len(palette_alpha) else 255)
            elif colour in (4, 6):
                out.append(px[-1])
            else:
                out.append(255)
        alpha.append(out)
    return width, height, alpha


def bitmap_glyphs(jar, provider):
    """Advance of each character a bitmap provider draws (`BitmapProvider`)."""
    path = "assets/minecraft/textures/" + provider["file"].split(":")[1]
    width, height, alpha = png_alpha(jar.read(path))
    grid = provider["chars"]
    cell_w, cell_h = width // len(grid[0]), height // len(grid)
    scale = provider.get("height", 8) / cell_h
    out = {}
    for row, chars in enumerate(grid):
        for col, ch in enumerate(chars):
            c = ord(ch)
            if c == 0:
                continue
            ink = 0
            for x in range(cell_w - 1, -1, -1):
                if any(alpha[row * cell_h + y][col * cell_w + x] for y in range(cell_h)):
                    ink = x + 1
                    break
            out[c] = int(0.5 + ink * scale) + 1
    return out


def providers(jar, name):
    """The default font's providers in order, references resolved."""
    out = []
    for p in json.loads(jar.read(f"assets/minecraft/font/{name}.json"))["providers"]:
        if p["type"] == "reference":
            out += [dict(q, filter=p.get("filter")) for q in providers(jar, p["id"].split(":")[1])]
        else:
            out.append(p)
    return out


def javap(jar, name):
    return subprocess.check_output(["javap", "-c", "-p", "-constants", "-classpath", str(jar), name], text=True)


def returned_int(code, signature):
    """The constant a one-line `int` getter returns."""
    blocks = [b for b in code.split("\n\n") if signature in b.strip().split("\n")[0]]
    if len(blocks) != 1:
        raise ValueError(f"expected one method matching {signature!r}, found {len(blocks)}")
    for line in blocks[0].splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1].startswith(("bipush", "sipush")):
            return int(parts[2])
    raise ValueError(f"no constant in {signature}")


def extract(client):
    data = client.read_bytes()
    if hashlib.sha1(data).hexdigest() != CLIENT_SHA1:
        raise ValueError("not the pinned official Java 26.3 client artifact")
    glyphs = {}
    with zipfile.ZipFile(client) as jar:
        sources = []
        for p in providers(jar, "default"):
            if p["type"] == "space":
                sources.append(("space", {ord(k): v for k, v in p["advances"].items()}, None))
            elif p["type"] == "bitmap":
                name = p["file"].split("/")[-1].removesuffix(".png")
                sources.append((name, bitmap_glyphs(jar, p), p))
        for c in codepoints():
            for name, advances, p in sources:
                if c in advances:
                    entry = {"advance": advances[c], "provider": name}
                    if p is not None:
                        entry["ascent"] = p["ascent"]
                        entry["height"] = p.get("height", 8)
                    glyphs[f"{c:04X}"] = entry
                    break
            else:
                glyphs[f"{c:04X}"] = {"provider": "unifont"}
    font = javap(client, "net.minecraft.client.gui.Font")
    line_height = int(font.split("public final int lineHeight = ")[1].split(";")[0])
    info = javap(client, "com.mojang.blaze3d.font.GlyphInfo")
    shadow = [b for b in info.split("\n\n") if "getShadowOffset()" in b.strip().split("\n")[0]][0]
    shadow_offset = {"fconst_0": 0, "fconst_1": 1, "fconst_2": 2}[shadow.split("0: ")[1].split()[0]]
    sign = javap(client, "net.minecraft.world.level.block.entity.SignBlockEntity")
    hanging = javap(client, "net.minecraft.world.level.block.entity.HangingSignBlockEntity")
    return {
        "reference": "Minecraft Java Edition 26.3",
        "client_sha1": CLIENT_SHA1,
        "source": "assets/minecraft/font/default.json and its includes, assets/minecraft/textures/font/*.png "
        "(widths only, as BitmapProvider measures them), Font.lineHeight, GlyphInfo.getShadowOffset, "
        "SignBlockEntity and HangingSignBlockEntity line metrics",
        "line_height": line_height,
        "shadow_offset": shadow_offset,
        "sign": {
            "max_line_width": returned_int(sign, "getMaxTextLineWidth()"),
            "line_height": returned_int(sign, "getTextLineHeight()"),
        },
        "hanging_sign": {
            "max_line_width": returned_int(hanging, "getMaxTextLineWidth()"),
            "line_height": returned_int(hanging, "getTextLineHeight()"),
        },
        "glyphs": glyphs,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("client", type=Path, help="official Java 26.3 client.jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.client), indent=1) + "\n")


if __name__ == "__main__":
    main()
