# Hosted agents and slash commands

[← Gameplay](gameplay.md)

A desktop host can now share its world with up to eight CLI-controlled players.
Agents have independent positions, inventories, health, hunger and creative
flight. Blocks, dropped items, chests, time and weather belong to the host.
The world continues ticking at 20 Hz while the host uses menus or loses focus.
This is the first agent multiplayer slice; desktop joining and gamepad
split-screen remain on the roadmap.

## Installed builds

The downloads include the `voxelcraft-agent` client beside the game:

- **Windows:** the Start Menu entry **VoxelCraft (host agent players)** opens
  the game hosting on `127.0.0.1:4242`. The client is installed as
  `%LOCALAPPDATA%\Programs\VoxelCraft\voxelcraft-agent.exe` (the default
  install folder).
- **Mac:** both programs are inside the app. Host from Terminal, then connect:

```sh
/Applications/VoxelCraft.app/Contents/MacOS/voxelcraft --agent-listen 127.0.0.1:4242
/Applications/VoxelCraft.app/Contents/MacOS/voxelcraft-agent --player builder observe
```

On Windows, run `voxelcraft-agent.exe --player builder observe` from that
folder in PowerShell or Command Prompt. Pick a world on the title screen first:
hosting starts once a world is loaded.

## Launch and connect from source

```sh
cargo run --release -- --world agents --agent-listen 127.0.0.1:4242
cargo run --release --no-default-features --bin voxelcraft-agent -- --player builder observe 2
```

After building, use `target/release/voxelcraft-agent` directly. Each invocation
rejoins the named player. Commands return one JSON object on stdout; errors
return `ok: false` and a one-shot invocation exits with status 1. Transport
errors go to stderr. No GPU, window or audio device is needed by the CLI.
With no command arguments it reads plain command lines from stdin:

```sh
printf 'observe\nlook 0 -30\nmove 1 0 20\nobserve 2\nleave\n' |
  target/release/voxelcraft-agent --player builder
```

A timed command responds after its last **real** simulation tick. `move` and
`mine` accept 1–200 ticks (up to ten seconds); agents cannot fast-forward the
shared world. Look angles are degrees: yaw 0 faces +X, yaw 90 faces +Z;
positive pitch looks up. Wait until `state.loaded` is true before moving.
`observe 1` or `observe 2` includes nearby cells; null cells are unloaded.

| Command | Effect |
|---|---|
| `help`, `players` | Command reference or active agent list |
| `catalog [query]` | Search available items; returns names, command names, IDs and stack limits |
| `observe [0..2]` | Position, dimension, health, hunger, inventory, target and optional nearby cells |
| `look yaw pitch` | Set camera direction |
| `move forward right ticks [jump sprint sneak]` | Physics input; axes −1 through 1; sneak also descends in flight |
| `wait ticks`, `mine ticks` | Wait or hold mining with normal tool speed, harvest rules and wear |
| `eat` | Eat one of the selected food over 1.6 seconds (32 ticks), when hungry and in survival |
| `select 1..9`, `fly on/off` | Choose hotbar slot or toggle creative flight |
| `place`, `attack`, `drop` | Place selected block, hit a targeted mob, or drop selected stack |
| `craft item` | Craft once with available recipe alternatives; larger recipes require targeting a crafting table |
| `chest take/put slot` | Transfer a stack using chest slot 0–26; put uses selected hotbar slot |
| `respawn`, `leave` | Respawn a dead agent or deactivate the named profile |

Agents stay active between CLI invocations until `leave` or host shutdown.
Names are 1–24 ASCII letters, digits or underscores and identify saved
profiles (at most 32). Inventory, vitals, position and mode save with the world;
profiles start inactive after restarting. Two clients using the same name
control the same player. A player accepts one timed input at a time; observations
can still be queried while it runs. Different names act concurrently. Chest
transfers and crafting commit on the game thread, preventing simultaneous item
duplication. Chunks load around the union of players; overlapping growth/fire
ranges tick once. Agent-only chunks outside the host's view do not get meshes.

For creative building, start the host with `--agent-cheats`, then run:

```sh
target/release/voxelcraft-agent --player builder gamemode creative
target/release/voxelcraft-agent --player builder give stone 64
target/release/voxelcraft-agent --player builder fly on
```

Cheats additionally allow `tp x y z`, `setblock x y z block`, `time noon` and
`weather clear`. Cheats are disabled for agents unless explicitly enabled.
The local in-game console keeps the game's existing unrestricted debug controls.

## In-game console

Press **/**, **T**, or **`** to open the console. Enter executes a command,
Escape closes it, Up/Down browse history, and Tab completes command names.
Text entry consumes gameplay keys. Offline play pauses while the console is
open; hosted play continues. Try `/help`, `/give diamond_pickaxe`,
`/gamemode creative`, `/tp 0 150 0`, `/time night`, `/weather rain`,
`/setblock 4 150 0 glowstone` or `/dimension nether`.
`--open-console` starts with it visible for screenshots.
The command style follows [Minecraft's slash command interface](https://www.minecraft.net/en-us/article/minecraft-commands).

## Split-screen views

Watch agents play in split-screen next to your own view. Start the host with
`--split-screen builder` (up to three comma-separated names), or type
`/splitscreen builder` in the console; run it again with the same name to
stop following that player. `/splitscreen off` closes every extra view, and
`/splitscreen side` or `/splitscreen stacked` (the default) arranges two
views left/right or top/bottom. `--split-layout side` sets this at launch.

Three or four views use quarters, with three players' third view spanning the
bottom. The host keeps the top or left view, with its menus and inventory.
Each followed player gets a first-person view showing their held item, block
target and mining cracks. Their HUD shows their name, hotbar, health, armor,
hunger and air, plus a death message. Everyone else, the host included,
appears as a player model. Terrain around followed players loads and draws at
the full render distance however far apart you are. Profiles that are offline
drop out of the layout until they reconnect.

### Gamepad players

Press Start on a connected gamepad to join as a local player. Each controller
plays as the first free profile from Player2 to Player8, gets its own view,
and keeps that profile's inventory and position for next time. New profiles
start next to the host in the world's game mode. Up to three controllers can
play beside the keyboard player. A command-line agent can't take over a
profile that a controller is using.

| Control | Action |
| --- | --- |
| Left stick / right stick | Move / look |
| A | Jump (double-tap to fly in creative); respawn after death |
| RT | Mine (hold) and attack |
| LT | Open a door, gate, chest or crafting table; otherwise place the held block (hold to repeat) or eat held food |
| LB / RB | Previous / next hotbar slot |
| B | Drop the held stack |
| Left stick click | Sprint until you stop walking forward |
| Right stick click | Toggle sneaking (flying down while flying) |
| Y | Inventory and crafting |
| Start | Pause menu: resume or leave the game |

Menus open inside the player's own view while the world keeps running. Move
with the D-pad or left stick. A takes, places, swaps or merges a stack (like a
left click), X takes half or places one (like a right click), and Y moves a
stack across: between the hotbar and the rest of the inventory, onto or off
armor, or into or out of an open chest. LB/RB switch between the inventory and
crafting, which lists everything you can make right now (A crafts one; 3x3
recipes need a crafting table opened with LT). B closes, putting any held
stack back.

An unplugged controller freezes its player until it reconnects.
`/splitscreen off` keeps controller views. Controller players can't yet use
furnaces, buckets, beds or bows.

Sound is shared: each sound plays as heard by whichever player in a view is
nearest, so everyone hears what happens around them, including blocks that
agents and controller players break, place or eat. Ambience, underwater
muffling and footsteps still follow the keyboard player.

## LAN and raw protocol

Bind `--agent-listen 0.0.0.0:4242 --agent-token <at-least-16-characters>` to
accept LAN agents; connect with `--connect HOST_IP:4242 --token <same-token>`.
This is a local-network, plaintext control protocol. A token authenticates all
agent access with the same permissions. Use loopback for same-computer agents.

Tools may bypass the CLI and send newline-terminated JSON over TCP:

```json
{"version":1,"player":"builder","command":"observe 2","token":"optional-shared-token"}
```

Requests are bounded to 4096 bytes, command text to 1024 bytes, concurrent
connections to 16 and queued requests to 64. Each response contains `ok` and
`version`; gameplay responses also include the host's tick, time, weather and
player state. Unknown versions, malformed input and permission failures return
structured errors. Invalid JSON closes the connection after its error response;
valid JSON requests with command or permission errors can continue using it.
Timeouts and host shutdown end outstanding requests.

## Current limits

Agents share the host's active dimension and relocate alongside it when the
host travels. Agent simulation pauses during arrival and while its feet or
supporting terrain are unloaded; timed commands resume after loading.
Independent simultaneous dimensions and desktop client joining
are future work. Every player has a stable ID (the host is 0; agent profiles
keep theirs in the save and `players` reports them). Hostile mobs chase the
nearest survival player, host or agent; melee, skeleton arrows and explosions
hurt agents with armor, knockback and death drops. Mobs spawn around every
player with per-player caps and stay loaded while any player is near.
Environmental survival damage, death drops and pickups work.
The first CLI supports block placement, crafting and chests; doors/beds/ladders,
food use, bows, buckets, furnaces, armor controls, sleep and portal interactions
still require the desktop player. Agents cannot mine multi-cell doors/beds.
