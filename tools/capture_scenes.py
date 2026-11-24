#!/usr/bin/env python3
"""Expand and run the named before/after comparison scenes in tools/capture_scenes.json.

Each scene becomes one `voxelcraft --screenshot` run per variant (resolution and GUI
scale). Every run gets its own data folder, so saves never leak between scenes. The
PNGs and a manifest.json recording the commit and each scene's settings go in one
folder. scripts/boat-capture.sh runs this inside a Boat sandbox under Xvfb with Mesa's
software Vulkan (one X server for the whole run, so set DISPLAY first), so the images show appearance only, never performance.

    python3 tools/capture_scenes.py list
    python3 tools/capture_scenes.py args stone-ore-wall
    python3 tools/capture_scenes.py run --binary target/ci/voxelcraft --out DIR [--scene NAME|GROUP ...]
    python3 tools/capture_scenes.py run --binary B --out DIR --adhoc NAME -- GAME ARGS...
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCENES = ROOT / "tools" / "capture_scenes.json"
# Settings each scene records and may override; the rest of a scene is its set-up.
SETTINGS = (
    "seed",
    "mode",
    "size",
    "gui_scale",
    "time",
    "weather",
    "dimension",
    "render_distance",
    "animation_time",
    "wait",
    "hud",
    "gamerules",
)
# A capture that hasn't finished by then has hung (software rendering is slow, not this slow).
TIMEOUT_SECONDS = 900
# Folder dates in the world list: 2026-11-01 12:00 UTC, an hour apart.
SAVE_EPOCH = 1793534400


def load(path=SCENES):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def variants(data, scene):
    """The scene's settings, once per resolution and GUI-scale variant."""
    base = dict(data["defaults"])
    base.update({k: v for k, v in scene.items() if k in SETTINGS})
    out = []
    matrix = scene.get("matrix")
    if not matrix:
        return [("", base)]
    for size in matrix.get("size", [base["size"]]):
        for scale in matrix.get("gui_scale", [base["gui_scale"]]):
            settings = dict(base, size=size, gui_scale=scale)
            out.append(("-{}x{}-gui{}".format(size[0], size[1], scale), settings))
    return out


def fill_cells(fill):
    """[x1, y1, z1, x2, y2, z2, block] (inclusive corners) as single cells."""
    x1, y1, z1, x2, y2, z2, block = fill
    cells = []
    for y in range(min(y1, y2), max(y1, y2) + 1):
        for z in range(min(z1, z2), max(z1, z2) + 1):
            for x in range(min(x1, x2), max(x1, x2) + 1):
                cells.append("{},{},{},{}".format(x, y, z, block))
    return cells


def scene_args(scene, settings):
    """The game's command-line arguments for one variant, without --screenshot or --data-dir."""
    args = ["--no-update-check", "--mute", "--no-vsync"]
    if not scene.get("title_screen"):
        args += ["--world", "capture", "--new", "--seed", str(settings["seed"]), "--" + settings["mode"]]
    args += ["--size", "{}x{}".format(*settings["size"])]
    args += ["--gui-scale", str(settings["gui_scale"])]
    args += ["--rd", str(settings["render_distance"])]
    args += ["--animation-time", str(settings["animation_time"])]
    if not scene.get("title_screen"):
        args += ["--time", str(settings["time"]), "--weather", settings["weather"]]
        for rule in settings["gamerules"]:
            args += ["--gamerule", rule]
        if settings.get("dimension"):
            args += ["--dimension", settings["dimension"]]
        if scene.get("pose"):
            args += ["--pose", ",".join(str(v) for v in scene["pose"])]
        if settings["wait"]:
            args += ["--wait", str(settings["wait"])]
        if not settings["hud"]:
            args.append("--no-hud")
        # Fills first, so single placements can go on top of them.
        for fill in scene.get("fill", []):
            for cell in fill_cells(fill):
                args += ["--place", cell]
        for cell in scene.get("place", []):
            args += ["--place", cell]
        for mob in scene.get("spawn", []):
            args += ["--spawn", mob]
    args += scene.get("args", [])
    return args


def icon_args(save, data_dir, png):
    """The game run that takes a save's world icon: its first visit, from the spawn.

    `--world-icon` makes the scripted run take `icon.png` by the game's usual rule
    and wait for it; the run writes nothing else to the save."""
    icon = save["icon"]
    args = ["--no-update-check", "--mute", "--no-vsync", "--data-dir", str(data_dir), "--world", save["dir"]]
    args += ["--size", "{}x{}".format(*icon.get("size", [640, 360]))]
    args += ["--rd", str(icon.get("render_distance", 4)), "--animation-time", "7.3"]
    args += ["--time", str(icon.get("time", 0.25)), "--gamerule", "advance_time=false"]
    args += ["--gamerule", "advance_weather=false", "--weather", "clear"]
    if icon.get("pose"):
        args += ["--pose", ",".join(str(v) for v in icon["pose"])]
    return args + ["--world-icon", "--screenshot", str(png)]


def take_icons(binary, out, scene_name, saves):
    """Takes each save's icon once per scene (`saves[].icon`), kept for every variant."""
    folder = out / ".icons" / scene_name
    for save in saves:
        if "icon" not in save:
            continue
        kept = folder / (save["dir"] + ".png")
        if kept.exists():
            continue
        data_dir = folder / ".data"
        if data_dir.exists():
            shutil.rmtree(data_dir)
        write_saves(data_dir / "saves", [save])
        command = [str(binary)] + icon_args(save, data_dir, folder / "first-visit.png")
        folder.mkdir(parents=True, exist_ok=True)
        with open(folder / (save["dir"] + ".log"), "w", encoding="utf-8") as f:
            try:
                subprocess.call(command, stdout=f, stderr=subprocess.STDOUT, timeout=TIMEOUT_SECONDS)
            except subprocess.TimeoutExpired:
                pass
        icon = data_dir / "saves" / save["dir"] / "icon.png"
        if icon.exists():
            shutil.copyfile(icon, kept)
            print("took the icon of {}".format(save["dir"]), file=sys.stderr)
        else:
            print("FAILED to take the icon of {}".format(save["dir"]), file=sys.stderr)
        shutil.rmtree(data_dir, ignore_errors=True)


def write_saves(saves_dir, saves, icons=None):
    """Fake saves for the world list: `level.txt` alone, with fixed folder dates, and
    the icon `take_icons` took for a save that asks for one."""
    for i, save in enumerate(saves):
        folder = saves_dir / save["dir"]
        folder.mkdir(parents=True, exist_ok=True)
        lines = ["seed={}".format(save.get("seed", 42))]
        lines += ["{}={}".format(k, v) for k, v in sorted(save.get("props", {}).items())]
        level = folder / "level.txt"
        level.write_text("\n".join(lines) + "\n", encoding="utf-8")
        stamp = SAVE_EPOCH - 3600 * i
        os.utime(level, (stamp, stamp))
        icon = icons / (save["dir"] + ".png") if icons else None
        if icon and icon.exists():
            shutil.copyfile(icon, folder / "icon.png")


def select(data, names):
    """Scenes named, or in a named group; all of them when none are given."""
    if not names:
        return list(data["scenes"])
    picked = [s for s in data["scenes"] if s["name"] in names or s.get("group") in names]
    known = {s["name"] for s in data["scenes"]} | {s.get("group") for s in data["scenes"]}
    unknown = [n for n in names if n not in known]
    if unknown:
        raise SystemExit("unknown scene or group: " + ", ".join(unknown))
    return picked


def capture(binary, out, name, args, commit, saves=(), icons=None):
    """One screenshot run in its own data folder; returns the manifest entry."""
    data_dir = out / ".data" / name
    if data_dir.exists():
        shutil.rmtree(data_dir)
    if saves:
        write_saves(data_dir / "saves", saves, icons)
    png = out / (name + ".png")
    if png.exists():
        png.unlink()
    command = [str(binary), "--data-dir", str(data_dir), "--screenshot", str(png)] + args
    started = time.time()
    log = out / (name + ".log")
    with open(log, "w", encoding="utf-8") as f:
        try:
            code = subprocess.call(command, stdout=f, stderr=subprocess.STDOUT, timeout=TIMEOUT_SECONDS)
        except subprocess.TimeoutExpired:
            code = "timeout"
    seconds = round(time.time() - started, 1)
    ok = code == 0 and png.exists()
    print("{} {} ({} s)".format("captured" if ok else "FAILED", name, seconds), file=sys.stderr)
    if ok:
        log.unlink()
    entry = {"png": png.name if ok else None, "exit": code, "seconds": seconds, "args": args, "commit": commit}
    if not ok:
        entry["log"] = log.name
    return entry


def run(opts):
    data = load()
    out = Path(opts.out)
    out.mkdir(parents=True, exist_ok=True)
    manifest = {
        "commit": opts.commit,
        "renderer": opts.renderer,
        "note": "Software-rendered captures show appearance only, never performance.",
        "captures": {},
    }
    previous = out / "manifest.json"
    if previous.exists():
        manifest["captures"] = json.loads(previous.read_text(encoding="utf-8")).get("captures", {})
    failed = 0
    if opts.adhoc:
        args = opts.game_args
        if args and args[0] == "--":
            args = args[1:]
        entry = capture(opts.binary, out, opts.adhoc, args, opts.commit)
        manifest["captures"][opts.adhoc] = dict(entry, adhoc=True)
        failed += entry["png"] is None
    else:
        for scene in select(data, opts.scene):
            for suffix, settings in variants(data, scene):
                name = scene["name"] + suffix
                args = scene_args(scene, settings)
                saves = scene.get("saves", ())
                icons = None
                if any("icon" in save for save in saves):
                    take_icons(opts.binary, out, scene["name"], saves)
                    icons = out / ".icons" / scene["name"]
                entry = capture(opts.binary, out, name, args, opts.commit, saves, icons)
                entry.update(scene=scene["name"], group=scene.get("group"), settings=settings)
                entry["placements"] = len(scene.get("place", [])) + sum(
                    len(fill_cells(f)) for f in scene.get("fill", [])
                )
                manifest["captures"][name] = entry
                failed += entry["png"] is None
    previous.write_text(json.dumps(manifest, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    shutil.rmtree(out / ".data", ignore_errors=True)
    shutil.rmtree(out / ".icons", ignore_errors=True)
    return 1 if failed else 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list", help="print each scene's name, group and description")
    show = sub.add_parser("args", help="print the game arguments of a scene's variants")
    show.add_argument("names", nargs="+")
    go = sub.add_parser("run", help="capture scenes")
    go.add_argument("--binary", required=True)
    go.add_argument("--out", required=True)
    go.add_argument("--scene", action="append", default=[], help="a scene or group name (repeatable)")
    go.add_argument("--commit", default="unknown")
    go.add_argument("--renderer", default="unknown")
    go.add_argument("--adhoc", help="capture one run of the arguments after -- under this name")
    go.add_argument("game_args", nargs=argparse.REMAINDER)
    opts = parser.parse_args(argv)
    data = load()
    if opts.command == "list":
        for scene in data["scenes"]:
            print("{:28} {:12} {}".format(scene["name"], scene.get("group", ""), scene.get("description", "")))
        return 0
    if opts.command == "args":
        for scene in select(data, opts.names):
            for suffix, settings in variants(data, scene):
                print(scene["name"] + suffix + ": " + " ".join(scene_args(scene, settings)))
        return 0
    return run(opts)


if __name__ == "__main__":
    sys.exit(main())
