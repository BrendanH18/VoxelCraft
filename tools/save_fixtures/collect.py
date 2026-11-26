"""Turns one make-fixture.sh run into tests/fixtures/saves/<tag>/.

    python3 tools/save_fixtures/collect.py [--dest DIR] RUN_DIR

RUN_DIR is the unpacked ~/out/<tag> folder from a Boat sandbox. The world
files are copied unchanged; manifest.json records what the tag's run put in
the world, for tests/save_compat.rs to check. --dest puts the fixture in
DIR/<tag> instead (cycle-current.sh's live run). Light work: it only copies
files and reads the run's logs, so it may run on the agent's own machine.
"""

import argparse
import json
import re
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CHEST = [1, 150, 2]


def lines(path):
    return [line.split() for line in path.read_text().splitlines() if line.strip()]


def main(run, dest_root):
    run = Path(run)
    tag = run.name
    dest = Path(dest_root) / tag
    if dest.exists():
        shutil.rmtree(dest)
    (dest / "world").mkdir(parents=True)
    for name in ("level.txt", "chunks.bin"):
        shutil.copyfile(run / "world" / name, dest / "world" / name)
    if (run / "options.txt").exists():
        shutil.copyfile(run / "options.txt", dest / "options.txt")

    args = (run / "args.txt").read_text().splitlines()
    pairs = list(zip(args, args[1:]))
    placed = [{"name": n, "id": int(i), "pos": [int(x), int(y), int(z)]} for n, i, x, y, z in lines(run / "placed.txt")]
    give = [v for k, v in pairs if k == "--give"]
    wear = [v for k, v in pairs if k == "--wear"]
    spawn = [v for k, v in pairs if k == "--spawn"]
    agent = (run / "agent.log").read_text()
    kept, total = (int(v) for v in (run / "trim.txt").read_text().split())
    created = (run / "create.log").read_text()
    saved = re.search(r"saved (\d+) modified overworld chunks", created)
    reopened = (run / "reopen.log").read_text()
    loaded = re.search(r"seed \d+, (\d+) modified chunks", reopened)
    resaved = re.search(r"saved (\d+) modified overworld chunks", reopened)
    exits = dict(re.findall(r"^(create|reopen) exit (\d+)$", agent, re.M))
    manifest = {
        "tag": tag,
        "commit": (run / "commit.txt").read_text().strip(),
        "rustc": (run / "rustc.txt").read_text().strip(),
        "seed": 20261125,
        "player": [8.5, 152.0, 8.5],
        "placed": placed,
        "platform": [[x, 149, z] for x in range(3) for z in range(3)],
        "chest": {
            "pos": CHEST,
            "filled": "chest reached" in agent,
            "slots": {"0": ["diamond", 7], "4": ["iron_ingot", 12]},
        },
        "give": give,
        "wear": wear,
        "spawn": spawn,
        "drop": [v for k, v in pairs if k == "--drop"],
        "agent": {"name": "keeper", "holds": ["oak_log", 20]},
        "chunks": {"kept": kept, "saved": total, "kept_chunk_y": 4},
        "create_saved_chunks": int(saved.group(1)) if saved else None,
        "reopen_loaded_chunks": int(loaded.group(1)) if loaded else None,
        "reopen_saved_chunks": int(resaved.group(1)) if resaved else None,
        "create_exit": int(exits["create"]) if "create" in exits else None,
        "reopen_exit": int(exits["reopen"]) if "reopen" in exits else None,
    }
    (dest / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")
    size = sum(f.stat().st_size for f in dest.rglob("*") if f.is_file())
    print(f"{tag}: {len(placed)} blocks, {kept}/{total} chunks kept, {size} bytes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dest", default=str(ROOT / "tests" / "fixtures" / "saves"))
    parser.add_argument("run")
    options = parser.parse_args()
    main(options.run, options.dest)
