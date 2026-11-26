"""Turns one make-fixture.sh run into tests/fixtures/saves/<tag>/.

    python3 tools/save_fixtures/collect.py [--dest DIR] RUN_DIR

RUN_DIR is the unpacked ~/out/<tag> folder from a Boat sandbox. The world
folder is copied unchanged (level.txt and chunks.bin before 1.0; from 1.0
also region/ and level.txt_old); manifest.json records what the tag's run put
in the world, for tests/save_compat.rs to check. --dest puts the fixture in
DIR/<tag> instead (cycle-current.sh's live run). Light work: it only copies
files and reads the run's logs, so it may run on the agent's own machine.
"""

import argparse
import json
import re
import shutil
from pathlib import Path

from trim_regions import records

ROOT = Path(__file__).resolve().parents[2]
CHEST = [1, 150, 2]


def lines(path):
    return [line.split() for line in path.read_text().splitlines() if line.strip()]


def last(pattern, text):
    """The number the last line matching `pattern` names, or None."""
    found = re.findall(pattern, text)
    return int(found[-1]) if found else None


def first(*numbers):
    return next((n for n in numbers if n is not None), None)


def region_records(world):
    """(records, records with blocks) in a world's region files (v1.0 Track 1b).

    A record's payload starts with its version byte and position (13 bytes),
    then 1 if the chunk's blocks follow; the frame before it is 16 bytes."""
    total = blocks = 0
    for path in sorted((world / "region").glob("r.*.vxr")):
        for _slot, frame in records(path.read_bytes()):
            total += 1
            blocks += frame[16 + 13] == 1
    return total, blocks


def main(run, dest_root):
    run = Path(run)
    tag = run.name
    dest = Path(dest_root) / tag
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    shutil.copytree(run / "world", dest / "world")
    regions = (dest / "world" / "region").is_dir()
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
    reopened = (run / "reopen.log").read_text()
    # Before 1.0 the quit save logged the chunks.bin chunks it wrote and the
    # reopen the ones it read. From 1.0 (region files) the records on disk
    # after the first run are trim_regions.py's count, the reopen logs the
    # records its region tables name, and each save the records it queued.
    old_saved = r"saved (\d+) modified overworld chunks"
    new_saved = r"saved (\d+) overworld chunk records to .*, written\)"
    saved = total if regions else last(old_saved, created)
    loaded = first(
        last(r"region storage: (\d+) chunk records in the overworld", reopened),
        last(r"seed \d+, (\d+) modified chunks", reopened),
    )
    resaved = first(last(new_saved, reopened), last(old_saved, reopened))
    chunks = {"kept": kept, "saved": total, "kept_chunk_y": 4}
    if regions:
        kept_records, kept_blocks = region_records(dest / "world")
        assert kept_records == kept, f"trim kept {kept} records, the copy has {kept_records}"
        # The chunks the fixture's blocks load from: records with blocks.
        chunks = {"kept": kept_blocks, "records": kept, "saved": total, "kept_chunk_y": 4}
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
        "chunks": chunks,
        "create_saved_chunks": saved,
        "create_save_records": last(new_saved, created),
        "reopen_loaded_chunks": loaded,
        "reopen_saved_chunks": resaved,
        "create_exit": int(exits["create"]) if "create" in exits else None,
        "reopen_exit": int(exits["reopen"]) if "reopen" in exits else None,
    }
    (dest / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")
    size = sum(f.stat().st_size for f in dest.rglob("*") if f.is_file())
    print(f"{tag}: {len(placed)} blocks, {kept}/{total} {'records' if regions else 'chunks'} kept, {size} bytes")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--dest", default=str(ROOT / "tests" / "fixtures" / "saves"))
    parser.add_argument("run")
    options = parser.parse_args()
    main(options.run, options.dest)
