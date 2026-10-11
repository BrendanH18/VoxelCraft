# LAN play (first desktop slice)

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
   for the menu. **Q** drops items. Chest, furnace and 2×2/3×3 crafting clicks,
   right clicks, shift transfers and armor slots run on the host.

The local profile is stored in `lan-profile.txt` beside `saves`. Names contain
1–24 letters, digits or underscores. The same profile reconnects to its
inventory, progression, position and bed saved by the host. Two simultaneous
instances need different profile names. Profiles are offline identities,
without account authentication. Use this on a trusted local network.

Host console commands **/kick NAME** and **/pvp on|off** manage players.
PvP defaults off in this first slice and currently applies to melee. Allow
Cheats enables supported player commands such as `/give`, `/gamemode`, `/tp`,
`/xp` and `/effect`; world-management commands remain host-only.

The host's **Save and Quit to Title** saves all named players, including cursor
and crafting inputs, then sends `Server closed`. A leaving client never saves
or modifies the host's files. Connection and compatibility errors return to
Multiplayer with their reason. The gamepad and JSON-lines agent interfaces
continue to use their existing protocol; they cannot take control of an active
LAN profile.

Direct launch:

```sh
voxelcraft --world friends --host-lan 0.0.0.0:25565 --profile Host
voxelcraft --join 192.168.1.20:25565 --profile Alex
```

Port `0` selects a free port. Discovery sends VoxelCraft-tagged MOTD/port
announcements to Java's multicast group `224.0.2.60:4445` every 1.5 seconds;
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
sent its real position, so chunks already delivered are never dropped. Changed immutable
chunk snapshots are resent in TCP order, without client terrain generation or
client world ticking. This preserves extended block states and all world edits,
including fluid/fire/redstone updates, at the cost of resending an entire changed
chunk. There is no edit-delta protocol yet.

State packets currently carry bounded JSON within the binary frame: player
inventory/vitals/XP/effects and seat, open chest/furnace/crafting state,
time/weather, nearby mob poses, boats and minecarts, dropped items and arrow
visuals. Each mob's look (coat, markings, saddle and armor, collar, wool,
name, lead, fish variant, profession, held weapon) is sent when a client first
sees it and refreshed once a second; clients derive walk animation from the
replicated motion. Seated clients take their position from the host instead
of predicting walking, so boats, minecarts, pigs, striders and horses work. This hybrid schema is
versioned together with the binary framing; it is not a final all-binary state
schema. Mob/item/arrow snapshots are capped at 256 each within 160 blocks.

Sockets are nonblocking during gameplay. Each peer has at most 2 MiB queued;
chunk sending yields at 512 KiB backlog and sends at most 32 snapshots per
host tick. Polling limits bytes, frames and accepts. At most eight TCP sessions
and 32 saved profiles are accepted. An incomplete handshake expires after
5 seconds, inactive sockets after 15 seconds, and held input after 0.5 seconds.
Shutdown drains pending ordered frames with a 100 ms deadline before closing.

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

Desktop smoke checks run isolated host/client data directories on a y=150
platform. Captures in `target/lan-check/` verify the Open to LAN options,
Multiplayer fields, published host port and a real joining desktop rendering
host terrain/held items/chat. A protocol client exercises inventory cursor
return, cheat rejection, creative mining/placement, movement and chest
store/shift-take while the host pause menu stays open. The host and client
use distinct local profiles.

## Remaining parity work

This is a playable first slice, not complete Java LAN parity:

- All players occupy the host's current dimension. Remote portal entry gives
  a message; when the host travels, LAN players follow and resynchronize.
  Independent dimensions and remote-triggered dimension travel need P0 work.
- Brewing, enchanting, anvils, smithing and villager screens are not replicated;
  using one reports that it is unsupported. The recipe-book controls, outside
  cursor tossing and boat/minecart/mount storage are not supported for LAN clients.
- Common mobs, players, held items/armor, dropped items and arrows replicate;
  other projectiles, TNT/falling-block entities, XP-orb visuals, boss-fight
  structures, skin/nameplate drawing and full mob animation/equipment fields
  need further replication. Host-side damage and pickups still apply.
- Remote action sounds/particles are not yet broadcast as an event stream.
  Mining crack/eating/bow-progress overlays need replication. PvP projectile
  damage, sweeping, critical hits and thorns need parity work.
- Input is coalesced per server tick, with simple correction/replay rather than
  a jitter buffer. Reconnect creates a fresh subscription. Moving rapidly into
  uncached chunks waits for the authority; view distance is capped at eight.
- Old player save records remain compatible. Session rules (cheats/PvP/mode)
  are selected again when publishing. There is no save-directory lock or
  authenticated/encrypted transport; independent hosts must use different saves.
- Multi-machine macOS/Windows/Linux testing remains necessary. The automated
  loopback test exercises device-free agent actions; desktop puppet/container
  behavior also needs broader concurrent, death and portal regression scenarios.

Reference behavior: [Minecraft Wiki: Java menu screens](https://minecraft.wiki/w/Tutorial:Menu_screen/Java_Edition)
and [setting up LAN worlds](https://minecraft.wiki/w/Tutorial:Setting_up_a_LAN_world).
Minecraft Wiki pages were robots-blocked in this environment; indexed wiki
material confirms Open to LAN, the mode/cheats controls, discovery/Direct
Connect and continued simulation while a published host menu is open.
