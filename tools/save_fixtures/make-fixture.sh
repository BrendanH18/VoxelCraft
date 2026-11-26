#!/usr/bin/env bash
# make-fixture.sh NAME [DISPLAY_NUM] [PORT]: v1.0 Track 1's save fixture run, in a Boat sandbox.
#
# Starts the game built from one source tree under Xvfb with Mesa's software Vulkan, builds a
# small world (blocks of that build's era, a stone platform, a chest an agent player fills,
# inventory, worn armour, mobs, a dropped stack), saves it by closing the window (the game's
# save on quit), then opens a copy once more and closes it again: start, save, quit, reopen.
# Writes $VC_FIXTURE_OUT/NAME (default ~/out/NAME): world/ (the save, its chunks.bin cut to the
# fixture's own chunk records by trim_chunks.py), options.txt, the logs and what was placed,
# for collect.py. Never run it on the owner's machines: it builds and runs the game.
#
#   VC_FIXTURE_SRC     the source tree (default ~/t/NAME, a worktree build-tag.sh made)
#   VC_FIXTURE_TARGET  its cargo target directory with a `ci` build (default ~/tgt/NAME)
set -uo pipefail
tag=$1 dnum=${2:-91} port=${3:-4242}
here=$(cd "$(dirname "$0")" && pwd)
wt=${VC_FIXTURE_SRC:-$HOME/t/$tag}
export CARGO_TARGET_DIR=${VC_FIXTURE_TARGET:-$HOME/tgt/$tag}
bin=$CARGO_TARGET_DIR/ci
out=${VC_FIXTURE_OUT:-$HOME/out}/$tag
rm -rf "$out"; mkdir -p "$out"
cd "$wt"
# A throwaway example that prints the ids this build gives each name, or `-`.
mkdir -p examples && cp "$here/probe.rs" examples/fixture_probe.rs
cargo +1.100.0 build --profile ci --locked --example fixture_probe >"$out/probe-build.log" 2>&1
built=$?
rm -f examples/fixture_probe.rs
[ $built = 0 ] || { echo "probe build failed"; tail -20 "$out/probe-build.log"; exit 1; }
probe="$bin/examples/fixture_probe"

BLOCKS="stone granite polished_andesite cobblestone mossy_cobblestone oak_planks spruce_planks birch_planks jungle_planks acacia_planks dark_oak_planks mangrove_planks cherry_planks bamboo_planks pale_oak_planks crimson_planks warped_planks oak_log stone_bricks bricks glass white_stained_glass bookshelf crafting_table furnace iron_block gold_block diamond_block emerald_block lapis_block coal_block netherite_block white_wool red_wool terracotta white_concrete obsidian netherrack glowstone soul_sand nether_bricks quartz_block blackstone basalt shroomlight end_stone end_stone_bricks purpur_block end_rod prismarine prismarine_bricks dark_prismarine sea_lantern deepslate cobbled_deepslate deepslate_tiles tuff tuff_bricks chiseled_tuff calcite amethyst_block dripstone_block moss_block mud_bricks packed_mud sculk sculk_catalyst sculk_sensor honeycomb_block hay_block bone_block barrel smoker blast_furnace lectern grindstone stonecutter loom cartography_table fletching_table smithing_table composter beehive jukebox note_block piston observer hopper dropper dispenser redstone_lamp target daylight_detector crafter decorated_pot chiseled_bookshelf resin_bricks waxed_copper_block heavy_core beacon conduit respawn_anchor lodestone enchanting_table brewing_stand cauldron ender_chest trial_spawner vault spawner reinforced_deepslate"
ITEMS="diamond_sword oak_planks bread torch iron_pickaxe diamond iron_ingot oak_log cobblestone iron_helmet leather_boots"
MOBS="cow sheep pig chicken villager"

$probe block $BLOCKS >"$out/blocks.txt"
$probe item $ITEMS >"$out/items.txt"
$probe mob $MOBS >"$out/mobs.txt"
has_item() { grep -q "^$1 [0-9]" "$out/items.txt"; }

args=(--data-dir "$out/data" --world fixture --seed 20261125 --mute --rd 2 --pose 8.5,152,8.5,45,10
      --agent-listen 127.0.0.1:$port --agent-cheats)
grep -q -- '"--no-update-check"' src/main.rs && args+=(--no-update-check)
# The platform the agent stands on, and the chest it fills.
for x in 0 1 2; do for z in 0 1 2; do args+=(--place $x,149,$z,stone); done; done
args+=(--place 1,150,2,chest)
# The era's blocks: two apart so none powers or touches another, 16 to a row.
i=0
: >"$out/placed.txt"
while read -r name id; do
  [ "$id" = - ] && continue
  x=$(( (i % 16) * 2 )) z=$(( -4 - (i / 16) * 2 ))
  args+=(--place $x,150,$z,$name)
  echo "$name $id $x 150 $z" >>"$out/placed.txt"
  i=$((i+1))
done <"$out/blocks.txt"
for g in diamond_sword oak_planks,32 bread,5 torch,16 iron_pickaxe; do
  has_item "${g%%,*}" && args+=(--give "$g")
done
has_item iron_helmet && args+=(--wear iron_helmet)
has_item leather_boots && args+=(--wear leather_boots)
args+=(--xp 5 --drop cobblestone,3)
n=0
for m in cow sheep pig villager; do
  grep -q "^$m 1" "$out/mobs.txt" || continue
  args+=(--spawn "$m,$((4 + n * 3)),~,12"); n=$((n+1))
done
printf '%s\n' "${args[@]}" >"$out/args.txt"

# One X server for this run.
Xvfb :$dnum -screen 0 1280x720x24 -nolisten tcp >/dev/null 2>&1 &
xvfb=$!
trap 'kill $xvfb 2>/dev/null' EXIT
sleep 1
export DISPLAY=:$dnum TZ=UTC RUST_LOG=info
agent() { "$bin/voxelcraft-agent" --connect 127.0.0.1:$port --player keeper "$@"; }

close_game() {  # $1 pid
  local wins
  wins=$(xdotool search --name '' 2>/dev/null | while read -r w; do
    [ "$(xdotool getwindowpid "$w" 2>/dev/null)" = "$1" ] && echo "$w"; done)
  [ -z "$wins" ] && wins=$(xdotool search --name 'VoxelCraft' 2>/dev/null)
  python3 "$here/close_window.py" $wins
  for _ in $(seq 1 120); do kill -0 "$1" 2>/dev/null || return 0; sleep 1; done
  echo "game did not exit after the close request"; kill -9 "$1"; return 1
}

wait_agent() {
  for _ in $(seq 1 240); do agent players >/dev/null 2>&1 && return 0; sleep 1; done
  echo "agent port never answered"; return 1
}

"$bin/voxelcraft" "${args[@]}" >"$out/create.stdout" 2>&1 &
pid=$!
wait_agent || { kill -9 $pid; exit 1; }
sleep 8
{
  agent respawn
  agent gamemode creative
  agent tp 1.5 150 0.5
  agent wait 40
  ok=0
  for yaw in 90 0 180 270; do
    agent look $yaw -30
    agent give diamond 7 && agent chest put 0 && { ok=1; echo "chest reached at yaw $yaw"; break; }
    agent clear
  done
  [ $ok = 1 ] || echo "CHEST NOT REACHED"
  agent give iron_ingot 12 && agent chest put 4
  agent give oak_log 20
  agent observe
} >"$out/agent.log" 2>&1
sleep 15
close_game $pid
wait $pid; echo "create exit $?" >>"$out/agent.log"
cp "$out/data/voxelcraft.log" "$out/create.log" 2>/dev/null
mkdir -p "$out/world"
cp -a "$out/data/saves/fixture/." "$out/world/"
rm -f "$out/world/session.lock" "$out/world/icon.png"
# Keep the fixture's own chunks (chunk y 4, blocks 128-159), record for record.
python3 "$here/trim_chunks.py" "$out/data/saves/fixture/chunks.bin" "$out/world/chunks.bin" 4 >"$out/trim.txt"
cp "$out/data/saves/options.txt" "$out/options.txt" 2>/dev/null

# Reopen a copy once with the same build: start, load, save on quit.
rm -rf "$out/reopen"; cp -a "$out/data" "$out/reopen"
"$bin/voxelcraft" --data-dir "$out/reopen" --world fixture --mute --rd 2 --agent-listen 127.0.0.1:$port \
  $(grep -q -- '"--no-update-check"' src/main.rs && echo --no-update-check) >"$out/reopen.stdout" 2>&1 &
pid=$!
wait_agent || { kill -9 $pid; exit 1; }
sleep 8
close_game $pid
wait $pid; echo "reopen exit $?" >>"$out/agent.log"
cp "$out/reopen/voxelcraft.log" "$out/reopen.log" 2>/dev/null
rustc +1.100.0 --version >"$out/rustc.txt"
git -C "$wt" rev-parse HEAD >"$out/commit.txt" 2>/dev/null || echo "${VC_FIXTURE_COMMIT:-unknown}" >"$out/commit.txt"
du -sh "$out/world" "$out/world"/*
