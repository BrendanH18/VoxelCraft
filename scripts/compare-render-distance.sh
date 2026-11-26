#!/usr/bin/env bash
# Compare two builds' frame times at several render distances with the offscreen suite, on
# the reference Mac (v1.0 Track 2). One command:
#
#   scripts/compare-render-distance.sh
#
# builds v0.17 and the current commit with `--release`, each in its own temporary git
# worktree, copying each binary out of the shared target folder before the next build,
# runs `tools/benchmark_scenes.py --offscreen` with both binaries at render distances 8, 16
# and 24 (32-block chunks: 256, 512 and 768 blocks), and prints a table.
#
#   --base REF        the older build (default v0.17)
#   --head REF        the newer build (default HEAD; uncommitted changes are not included)
#   --rd "8 16 24"    render distances, in the game's 32-block chunks
#   --scenes LIST     comma-separated scenes both builds have (default survival,streaming,entities)
#   --repeats N       runs per scene (default 3)
#   --seconds S       sample seconds per run (default 60)
#   --out DIR         results (default target/benchmarks/<date>-render-distance)
#   --table DIR       only print the table of an earlier run's results
#
# These are comparison runs, never gate evidence: a render distance other than 8 and a
# binary built from another commit fail the gate's checks by design. Run on AC power with
# nothing else running; the builds and the runs take a while (about 1 h 10 min with the
# defaults: 2 builds, then 2 x 3 x 3 x 3 runs of 60 s plus loading).
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
root=$(pwd)
base=v0.17 head=HEAD rds="8 16 24" scenes=survival,streaming,entities repeats=3 seconds=60
out="" table_only=""
while [ $# -gt 0 ]; do
    case "$1" in
        --base) base=$2; shift ;;
        --head) head=$2; shift ;;
        --rd) rds=$2; shift ;;
        --scenes) scenes=$2; shift ;;
        --repeats) repeats=$2; shift ;;
        --seconds) seconds=$2; shift ;;
        --out) out=$2; shift ;;
        --table) table_only=$2; shift ;;
        -h|--help) sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        *) echo "compare-render-distance: unknown option $1 (try --help)" >&2; exit 2 ;;
    esac
    shift
done

table() {
    python3 - "$1" <<'PY'
import json
import sys
from pathlib import Path

out = Path(sys.argv[1])
labels = json.loads((out / "labels.json").read_text())
rows = {}
for label in ("base", "head"):
    for summary in sorted((out / label).glob("rd*/summary.json")):
        rd = int(summary.parent.name[2:])
        for run in json.loads(summary.read_text())["runs"]:
            stats = run["frame_stats"]
            gpu = ((run.get("gpu_timestamp_ms") or {}).get("all_frames") or {}).get("gpu_ms") or {}
            key = (run["scene"], rd)
            rows.setdefault(key, {}).setdefault(label, []).append(
                (stats["average_fps"], stats["one_percent_low_fps"], stats["p99_ms"], gpu.get("p99"))
            )


def mean(values):
    values = [v for v in values if v is not None]
    return sum(values) / len(values) if values else None


def cell(v, digits=1):
    return "-" if v is None else f"{v:.{digits}f}"


print(f"base = {labels['base']}, head = {labels['head']}; means over each scene's runs")
print("| Scene | RD | Base avg FPS | Base 1% low | Base p99 ms | Base GPU p99 ms | Head avg FPS | Head 1% low | Head p99 ms | Head GPU p99 ms | 1% low change |")
print("|---|---|---|---|---|---|---|---|---|---|---|")
for (scene, rd), by in sorted(rows.items()):
    b = [mean(col) for col in zip(*by.get("base", [(None,) * 4]))]
    h = [mean(col) for col in zip(*by.get("head", [(None,) * 4]))]
    change = "-" if not (b[1] and h[1]) else f"{(h[1] / b[1] - 1) * 100:+.1f}%"
    print(
        f"| {scene} | {rd} | {cell(b[0])} | {cell(b[1])} | {cell(b[2], 2)} | {cell(b[3], 2)} | "
        f"{cell(h[0])} | {cell(h[1])} | {cell(h[2], 2)} | {cell(h[3], 2)} | {change} |"
    )
PY
}

if [ -n "$table_only" ]; then
    table "$table_only"
    exit 0
fi

base_commit=$(git rev-parse --verify "$base^{commit}")
head_commit=$(git rev-parse --verify "$head^{commit}")
out=${out:-target/benchmarks/$(date +%F)-render-distance}
mkdir -p "$out/bin"
out=$(cd "$out" && pwd)
printf '{"base": "%s (%s)", "head": "%s (%s)"}\n' "$base" "$base_commit" "$head" "$head_commit" > "$out/labels.json"

# Both builds share one target folder (the second replaces the first's binary), so each
# binary is copied out before the next build.
build() {
    local label=$1 commit=$2
    local tree="$root/target/compare-render-distance/src-$label"
    git worktree remove --force "$tree" 2>/dev/null || true
    git worktree add --detach "$tree" "$commit" >/dev/null
    echo "building $label ($commit)" >&2
    (cd "$tree" && CARGO_TARGET_DIR="$root/target/compare-render-distance/target" cargo build --release --locked --bin voxelcraft)
    cp "$root/target/compare-render-distance/target/release/voxelcraft" "$out/bin/voxelcraft-$label"
    git worktree remove --force "$tree"
}
build base "$base_commit"
build head "$head_commit"

awake=()
command -v caffeinate >/dev/null && awake=(caffeinate -d)
for rd in $rds; do
    for label in base head; do
        echo "== $label at render distance $rd" >&2
        ${awake[@]+"${awake[@]}"} python3 tools/benchmark_scenes.py --executable "$out/bin/voxelcraft-$label" --offscreen \
            --scenes "$scenes" --repeats "$repeats" --seconds "$seconds" --rd "$rd" --output "$out/$label/rd$rd" \
            || echo "compare-render-distance: $label at render distance $rd failed; see $out/$label/rd$rd" >&2
    done
done
table "$out" | tee "$out/table.md"
echo "results: $out" >&2
