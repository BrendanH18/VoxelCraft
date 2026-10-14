# LAN play

[← VoxelCraft](../README.md)

VoxelCraft desktop instances can share a world over TCP. This is VoxelCraft's
own protocol; it does not connect to vanilla Minecraft servers.

## Host and join

1. Load a world, press **Esc → Open to LAN**, select the joining players'
   Survival/Creative mode, Allow Cheats and PvP, then **Start LAN World**.
   The pause menu and chat show the chosen port. The world keeps ticking
   while the host's menus are open after publishing, as in Java Edition.
2. On another instance, select **Multiplayer**. A discovered world fills in
   its address when clicked. Alternatively type the host's LAN IP and port
   into Server Address, choose a local profile and press **Direct Connect**.
3. Use normal movement, left-click mining/combat, right-click use/place,
   **E** for inventory, **T** for chat, **Tab** for the player list and **Esc**
   for the menu. **Q** drops items. Every screen works: chests and barrels,
   furnaces, crafting, brewing stands, enchanting tables, anvils, grindstones,
   smithing tables, villager and wandering-trader trades, and boat, minecart
   and mount storage. Clicks, right clicks, shift transfers, armor slots and
   throwing the held stack out of a screen run on the host; the recipe book
   and search are local views.

The Multiplayer list shows each discovered world's name, address and player
count, and flags a world that needs a different VoxelCraft version.

The local profile is stored in `lan-profile.txt` beside `saves`. Names contain
1–24 letters, digits or underscores. The same profile reconnects to its
inventory, progression, position and bed saved by the host. Two simultaneous
instances need different profile names. Profiles are offline identities,
without account authentication. Use this on a trusted local network.

Host console commands **/kick NAME** and **/pvp on|off** manage players, and
**/save-all** saves the world (LAN players can use it too when cheats are on).
PvP defaults off. With it on, players fight with Java's rules: criticals while
falling, enchantment bonuses, Knockback and sprint shoves, Fire Aspect, sword
sweeps, Thorns, and arrows with Power, Punch and Flame. Allow Cheats enables
supported player commands such as `/give`, `/gamemode`, `/tp`, `/xp` and
`/effect`; world-management commands remain host-only.

Players see each other's names (within 64 blocks, hidden while sneaking), arm
swings, eating and block-breaking cracks, and hear each other's footsteps,
blocks and fights.

The host's **Save and Quit to Title** saves all named players, including cursor
and crafting inputs, then sends `Server closed`. A leaving client never saves
or modifies the host's files. Connection and compatibility errors return to
Multiplayer with their reason. The gamepad and JSON-lines agent interfaces
continue to use their existing protocol; they cannot take control of an active
LAN profile.

Direct launch:

```sh
voxelcraft --world friends --host-lan 0.0.0.0:25565 --profile Host [--lan-cheats] [--lan-pvp]
voxelcraft --join 192.168.1.20:25565 --profile Alex
```

A world can only be open in one VoxelCraft process at a time: opening it claims
`session.lock` in the save folder (released when the game exits, even after a
crash), and a second window is sent back to the world list.

Port `0` selects a free port. Discovery sends VoxelCraft-tagged MOTD/port,
player-count and version announcements to Java's multicast group `224.0.2.60:4445` every 1.5 seconds;
entries expire after 5 seconds. Direct Connect supports hostname, IPv4 and
bracketed IPv6 addresses. Discovery may be unavailable when another process
has bound UDP 4445, or when multicast is filtered; Direct Connect still works.

## Authority and wire format

The desktop host alone advances survival, mobs, world systems and inventories
at 20 Hz. Joining clients render a replica and send movement/look/button input.
Mouse press/release edges are queued in order so short taps survive movement
coalescing. Remote hands use `Game::puppet_body`, the same mining/use/combat rules as local
controllers. Slot clicks are bounded and checked against the actual host screen;
container reach and block type are rechecked before transfer. Client prediction
uses the engine's collision physics; acknowledged input is removed, remaining
inputs replay from the authoritative position/velocity. Remote player/mob
positions and arrows interpolate between snapshots.

Protocol **1** requires an exact game-version match. Each frame starts with a
little-endian `u32` payload length, followed by a packet byte. Maximum payload:
256 KiB. Hello includes a `u16` protocol version, game version and profile;
Welcome supplies seed, dimension and player ID. Input contains a sequence,
four `f32` values (yaw/pitch/forward/right), button bits and selected slot.
Nonfinite values, movement outside −1…1, unknown buttons/slots, malformed RLE
and out-of-range block states are rejected. Text is bounded to 1024 bytes.

Chunk packets contain three `i32` coordinates and `(u16 length, u16 block
state)` RLE pairs totaling exactly 32³ cells. After Welcome the client asks
for its own render distance; the host streams the smaller of that and its own,
capped at 8 chunks (256 blocks, Java's 16), plus two rings so edge chunks have
all their meshing neighbours, nearest first. The host keeps that radius loaded
around every remote player. A joining client evicts nothing until the host has
sent its real position, so chunks already delivered are never dropped. When a
chunk the client has changes, the host sends just the changed cells (an Edits
packet: chunk position and `(u16 index, u16 state)` pairs) unless more than
1024 changed, and the whole chunk otherwise. Clients never generate terrain or
tick the world; fluids, fire, redstone and edits all arrive this way.

State packets carry bounded JSON within the binary frame, versioned with the
binary framing:

- the player's own state (inventory, vitals, XP, effects, seat) and its open
  screen (container contents, work slots, brewing stand, merchant offers or
  vehicle storage);
- other players as a lite pose: position, look, held item, armor, health,
  sneaking, seat, arm swing and eating;
- time and weather; nearby mobs as compact rounded arrays, with each mob's look
  (coat, markings, saddle, armor, carpet, collar, wool, name, lead, fish
  variant, profession, held weapon, mount health) on first sight and once a
  second, and mount jump charge every tick; clients derive walk animation from
  the motion;
- boats and minecarts, dropped items, arrows, thrown pearls, potions,
  snowballs, eggs and llama spit, fishing bobbers, eyes of ender, fireballs,
  dragon breath, XP orbs, primed TNT, falling blocks and the Ender Dragon fight
  (pose, health for the boss bar, crystals and beams, fireballs);
- sounds (dense bank keys) and particle requests near the player, and every
  player's block-breaking progress within 32 blocks.

Seated clients take their position from the host instead of predicting walking,
so boats, minecarts, pigs, striders, horses, llamas and camels work. Snapshots
are capped (256 mobs, items and arrows, 128 of each other entity) within 160
blocks; a crowded frame sheds extras, items, mob looks and then mobs rather than
exceeding 256 KiB.

Sockets are nonblocking during gameplay. Each peer has at most 2 MiB queued;
chunk sending yields at 512 KiB backlog and sends at most 32 new chunks per
host tick. Polling limits bytes, frames and accepts. At most eight TCP sessions
and 32 saved profiles are accepted. An incomplete handshake expires after
5 seconds, inactive sockets after 15 seconds, and held input after 0.5 seconds.
Shutdown drains pending ordered frames with a 100 ms deadline before closing.
While clients are connected the host logs `LAN stats` every ten seconds: tick
time and bandwidth.

## Headless verification

The device-free protocol exerciser uses real `World` streaming, `Agent` physics
and validated actions. Its world is temporary, and its smaller state schema is
marked `harness`; it is for tests, not a dedicated desktop server. Desktop
clients reject it with an explicit explanation.

```sh
cargo run --release --no-default-features --bin voxelcraft-lan -- \
  --host-lan 127.0.0.1:25566 --ticks 400
cargo run --release --no-default-features --bin voxelcraft-lan -- \
  --join 127.0.0.1:25566 --profile Loopback --ticks 100
cargo test --release --test lan_loopback -- --nocapture
```

The in-process loopback test opens real TCP connections with no window/GPU,
checks protocol/version rejection, exact initial chunk replication (including
extended block states), a validated placement replicated back to the client,
cheat rejection, a second client seeing the same edit, and clean disconnect.
Codec tests cover fragmented frames, bounded queues and malformed input;
desktop adapter tests cover slot context and player-state round trips.

On an Apple M5/macOS release build (2026-10-10), the isolated flat-platform
loopback fixture reached its first authoritative chunk in **2.55 ms**, with
**601 bytes** received at that point; placement round-trip was **12.93 ms**.
The full handshake/edit/cheat-rejection exercise received **29,093 bytes** and
sent **74 bytes**. Terrain workers run concurrently, so totals vary with the
number of additional chunks completing during the exercise. These figures
measure the fixture, not completion of a full rendered-world join.

A separate-process, generated-world harness run at 20 Hz reached handshake /
first chunk in **56.27 / 56.29 ms** (the CLI polls every 50 ms). Over the
40-poll/two-second client run it received **612 chunk snapshots**,
**3,527,700 bytes** and sent **1,055 bytes**: about **1.76 MB/s** including
initial terrain transfer. This is initial-load bandwidth, not steady state.

## Desktop acceptance checks

`tools/lan_check.py` runs a real desktop host in a throwaway data folder and
drives protocol clients through the P0A scenarios (needs a display and a
release build):

```sh
cargo build --release && python3 tools/lan_check.py
```

It checks join timing; two players racing for one chest stack, one block and
its drop (items are conserved, the drop is picked up once); death and respawn;
reconnecting to the same profile; a host restart from `/save-all`; portal entry
by a LAN player; a 2.8 km teleport streaming new terrain; joining while water,
lava, fire and primed TNT are active; PvP on and off; and the host traveling
through a portal with its players following into the Nether.

Measured on an Apple M5 (macOS, release build, 2026-10-12, loopback):

| | |
|---|---|
| Handshake / first chunk / first state | 4–21 ms / 31–79 ms / 32–79 ms |
| Steady-state traffic, one idle client at spawn | about 27 KiB/s (under 1 KiB/s of it block edits) |
| Two active clients, one streaming new terrain | 50–240 KiB/s per client |
| Host tick with two clients | 3.6–5.5 ms average, 11–17 ms worst |
| New terrain after a 2.8 km teleport | 300+ chunks in 0.5 s |
| Joining a busy area (fluids, fire, TNT) | first state in 30–50 ms |
| Following the host to the Nether | 5–6 s (the 4 s portal wait plus loading) |

## Current limits

- Everyone shares the host's dimension, as all LAN players follow the host
  through portals; a LAN player stepping into a portal is told travel is
  host-controlled. Independent per-player dimensions come with the dedicated
  server (P0/P1).
- Profiles are offline identities on a trusted network: there are no accounts,
  and the transport is neither authenticated nor encrypted. This connects
  VoxelCraft to VoxelCraft only, not to Minecraft servers.
- Input is coalesced per server tick with correction and replay rather than a
  jitter buffer, so high-latency links (beyond a home LAN) will feel rubbery.
- Cheats and PvP are remembered in the world's save (`lan_rules`) and are the
  defaults the next time it opens; the game mode is the host's own saved mode.
- Automated checks run separate processes on one Mac; play across real
  Windows, macOS and Linux machines on a home network still needs a hands-on
  pass.

Reference behavior: [Minecraft Wiki: Java menu screens](https://minecraft.wiki/w/Tutorial:Menu_screen/Java_Edition)
and [setting up LAN worlds](https://minecraft.wiki/w/Tutorial:Setting_up_a_LAN_world).
Minecraft Wiki pages were robots-blocked in this environment; indexed wiki
material confirms Open to LAN, the mode/cheats controls, discovery/Direct
Connect and continued simulation while a published host menu is open.
