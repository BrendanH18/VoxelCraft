#!/usr/bin/env python3
"""Validate original asset provenance, formats, references and packaged embedding."""

import argparse
import json
import struct
import subprocess
import tempfile
import wave
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def validate():
    manifest = json.loads((ROOT / "assets/manifest.json").read_text())
    if not manifest["schema"] == 1: raise AssertionError("unsupported manifest schema")
    entries = {e["key"]: e for e in manifest["resources"]}
    if not len(entries) == len(manifest["resources"]): raise AssertionError("duplicate resource key")
    layers = set()
    for key, entry in entries.items():
        if not entry["license"] == "MIT OR Apache-2.0": raise AssertionError(f"{key}: review asset license")
        if not entry["provenance"].strip(): raise AssertionError(f"{key}: missing provenance")
        path = ROOT / entry["path"]
        if not (path.is_relative_to(ROOT / "assets") and ".." not in path.parts): raise AssertionError(f"{key}: invalid path")
        if not path.is_file(): raise AssertionError(f"{key}: missing {entry['path']}")
        if entry["kind"] == "texture":
            blob = path.read_bytes()
            if blob[:8] != b"\x89PNG\r\n\x1a\n": raise AssertionError(f"{key}: not PNG")
            at, compressed = 8, bytearray()
            while at < len(blob):
                size = struct.unpack(">I", blob[at : at + 4])[0]
                kind, data = blob[at + 4 : at + 8], blob[at + 8 : at + 8 + size]
                crc = struct.unpack(">I", blob[at + 8 + size : at + 12 + size])[0]
                if zlib.crc32(kind + data) & 0xFFFFFFFF != crc: raise AssertionError(f"{key}: invalid PNG checksum")
                if kind == b"IHDR":
                    if struct.unpack(">IIBBBBB", data) != (16, 16, 8, 6, 0, 0, 0): raise AssertionError(f"{key}: require 16 x 16 RGBA8")
                if kind == b"IDAT":
                    compressed.extend(data)
                at += size + 12
            if len(zlib.decompress(compressed)) != 16 * (1 + 16 * 4): raise AssertionError(f"{key}: truncated pixels")
            layer = entry["texture_layer"]
            if not (0 <= layer < 4096 and layer not in layers): raise AssertionError(f"{key}: invalid/duplicate texture layer")
            layers.add(layer)
            if "source" in entry:
                source = json.loads((ROOT / entry["source"]).read_text())
                if not (len(source["pixels"]) == 16 and all(len(r) == 16 for r in source["pixels"])):
                    raise AssertionError(f"{key}: invalid authoring source")
                
        elif entry["kind"] == "model":
            model = json.loads(path.read_text())
            if not (model["schema"] == 1 and model["texture"] in entries): raise AssertionError(f"{key}: invalid model reference")
            if not (1 <= len(model["boxes"]) <= 10): raise AssertionError(f"{key}: model box capacity exceeded")
            for box in model["boxes"]:
                if not (len(box["min"]) == len(box["max"]) == 3): raise AssertionError(f"{key}: invalid box coordinates")
                if not (all(0 <= lo < hi <= 16 for lo, hi in zip(box["min"], box["max"]))): raise AssertionError(f"{key}: invalid bounds")
            # Static block-model format. Entity animation stays in the existing Rust builders.
            if not (model.get("animations", {}) == {}): raise AssertionError(
                f"{key}: unsupported animation; add schema and runtime support first"
            )
        elif entry["kind"] == "audio":
            with wave.open(str(path), "rb") as wav:
                if not (wav.getnchannels(), wav.getsampwidth(), wav.getframerate(), wav.getcomptype()) == (
                    1,
                    2,
                    48000,
                    "NONE",
                ):
                        raise AssertionError(f"{key}: invalid PCM format")
                if not 0 < wav.getnframes() <= 48000 * 30:
                        raise AssertionError(f"{key}: unbounded audio sample")
                wav.readframes(wav.getnframes())
        else:
            raise ValueError(f"{key}: unsupported resource kind {entry['kind']}")
    return len(entries)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--executable", type=Path, help="also check a development or packaged binary from outside the checkout"
    )
    args = parser.parse_args()
    count = validate()
    if args.executable:
        executable = args.executable.resolve(strict=True)
        with tempfile.TemporaryDirectory(prefix="voxelcraft-asset-check-") as cwd:
            result = subprocess.run(
                [str(executable), "--validate-assets"], cwd=cwd, check=True, text=True, capture_output=True
            )
        metadata = json.loads(result.stdout)
        if metadata["resources"] != count: raise AssertionError("packaged executable has an outdated asset manifest")
        print(json.dumps(metadata, indent=2))
    else:
        print(f"{count} original resources validated")


if __name__ == "__main__":
    main()
