#!/usr/bin/env python3
"""Extract Java 26.3's Mipmap Levels option from the pinned client jar.

Writes tests/fixtures/java/26.3/mipmap_levels.json: the label, the options.txt
key, the slider's range and default, its value captions, what a change does,
and the Quality & Performance options of Video Settings in order. Reads the
bytecode for numbers and order only.

Usage: extract_java_mipmap_fixtures.py client.jar --output tests/fixtures/java/26.3/mipmap_levels.json
(`javap` must be on PATH).
"""

import argparse
import hashlib
import json
import re
import subprocess
import zipfile
from pathlib import Path

from extract_java_registry_names import CLIENT_SHA1

OPTIONS = "net.minecraft.client.Options"
VIDEO = "net.minecraft.client.gui.screens.options.VideoSettingsScreen"


def javap(jar, name, *flags):
    return subprocess.check_output(["javap", "-c", "-p", *flags, "-classpath", str(jar), name], text=True)


def method_body(code, signature):
    """The bytecode of the first method whose declaration contains `signature`."""
    start = code.index(signature)
    end = code.find("\n\n", start)
    return code[start : end if end >= 0 else len(code)]


def lambda_for(verbose, index):
    """The synthetic method bootstrap method `index` points at."""
    table = verbose.split("BootstrapMethods:", 1)[1]
    entry = re.search(r"\n\s+{}: #\d+ .*?\n(.*?)(?=\n\s+\d+: #|\Z)".format(index), table, re.S)
    name = re.search(r"REF_\w+ [\w/$]+\.(lambda\$[\w$]+):", entry.group(1))
    return name.group(1)


def option_facts(client):
    verbose = javap(client, OPTIONS, "-v")
    init = method_body(verbose, "public net.minecraft.client.Options(")
    block = init.split("// String options.mipmapLevels\n", 1)[1]
    block = block.split("// Field mipmapLevels:", 1)[0]
    ints = [int(v) for v in re.findall(r"iconst_(\d)", block)]
    if "OptionInstance$IntRange" not in block or len(ints) != 3:
        raise ValueError("Mipmap Levels is no longer an IntRange with a default")
    low, high, default = ints
    caption = lambda_for(verbose, int(re.search(r"InvokeDynamic #(\d+):toString", block).group(1)))
    listener = lambda_for(verbose, int(re.search(r"InvokeDynamic #(\d+):valueChanged", block).group(1)))
    caption_body = method_body(verbose, caption + "(")
    if "optionStatus" not in caption_body or "genericValueLabel" not in caption_body or "ifne" not in caption_body:
        raise ValueError("Mipmap Levels captions changed")
    listener_body = method_body(verbose, listener + "(")
    save = method_body(verbose, "processDumpedOptions(")
    key = re.search(r"// String (\w+)\n[^\n]*\n[^\n]*// Field mipmapLevels:", save)
    if not key:
        raise ValueError("Mipmap Levels' options.txt key not found")
    return {
        "key": key.group(1),
        "min": low,
        "max": high,
        "default": default,
        "marks_graphics_preset_custom": "setGraphicsPresetToCustom" in listener_body,
    }


def quality_order(client):
    code = method_body(javap(client, VIDEO), "qualityOptions(")
    return re.findall(r"Method net/minecraft/client/Options\.(\w+):", code)


def extract(client):
    data = client.read_bytes()
    if hashlib.sha1(data).hexdigest() != CLIENT_SHA1:
        raise SystemExit("expected the Java 26.3 client jar (SHA-1 {})".format(CLIENT_SHA1))
    with zipfile.ZipFile(client) as jar:
        lang = json.loads(jar.read("assets/minecraft/lang/en_us.json"))
    facts = option_facts(client)
    order = quality_order(client)
    return {
        "source": "Java 26.3 client: Options, VideoSettingsScreen and en_us.json",
        "client_sha1": CLIENT_SHA1,
        "mipmapLevels": dict(
            label=lang["options.mipmapLevels"],
            # 0 shows the option status, "label: OFF"; other values the generic "label: value".
            zero_format=lang["options.off.composed"],
            value_format=lang["options.generic_value"],
            **facts,
        ),
        "quality_options": order,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("client", type=Path, help="official Java 26.3 client.jar")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = extract(args.client)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
