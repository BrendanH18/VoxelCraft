# Redstone

Redstone simulation runs once per shared 50 ms game tick, including headless
worlds. Component block states occupy the append-only 1100–1499 allocation;
existing redstone dust (item 365) places wire. States share texture layers.

## Signal engine

Signals have integer strength 0–15. Strong outputs power one full conducting
block; weak outputs activate adjacent devices without conducting through it.
Wire excludes all wire emissions while reading external power, then takes the
maximum connected wire power minus one. Horizontal wires step up a conducting
block when their own ceiling is clear, and down beside nonconductors. Wire
strongly powers its supporting block and the horizontal solids it points at.
A redstone block is a weak source; a torch strongly powers the block above it.

An iterative, deduplicated FIFO handles neighbour notifications. An edit wakes
only a fixed two-block neighbourhood. A heap ordered by deadline, priority and
insertion sequence handles scheduled ticks. Idle circuits perform no block
scans, allocate no memory and produce no work. A 65,536-update budget carries
pathological feedback work to the next tick instead of recursing or hanging.

Each tick increments the redstone clock, drains queued notifications, processes
due scheduled ticks in priority order and drains notifications after each tick.
Repeaters turn off at very high priority and on at high priority. This differs
from Java's synchronous neighbour callback stack and its directional diode
priority heuristic; update-order-sensitive contraptions are not guaranteed.
Timing otherwise uses Java game ticks: torch inversion 2, repeater delay
2/4/6/8, comparator 2, lamp off 4, stone button 20, wooden button 30. Scheduled
repeater rising edges stretch short input pulses. Only side-facing repeaters
and comparators lock repeaters. Eight torch off transitions within 60 ticks
cause a 160-tick restart delay. Burnout history is bounded per torch.

Wire power, facing, delay and on/off states are in VXC2 block data. A versioned
`redstone` dimension property preserves queued work, pending deadlines,
comparator output, torch history/burnout, and powered door/gate edges. Replaced
blocks invalidate their old ticks. Unloaded scheduled blocks wait for loading;
this currently retries their deadlines every tick instead of Java chunk tick
containers. Old saves without the property remain readable. Connections are
recomputed from neighbours rather than saved.

Comparators use Java inventory fullness (floor(14 × average normalized slot
fullness) plus one for any nonempty slot), reading chests, furnaces and brewing
stands directly or through one conductor. Item frames and double chests are
not implemented. Container mutable access wakes adjacent comparators.
Wooden doors and fence gates respond to rising/falling power edges, preserving
manual operation while power stays unchanged. Powered TNT primes once.

## Java sources

Behaviour is checked against the Minecraft Java block implementations:

- [RedStoneWireBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RedStoneWireBlock.java)
- [RedstoneTorchBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RedstoneTorchBlock.java)
- [DiodeBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/DiodeBlock.java)
- [RepeaterBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/RepeaterBlock.java)
- [ComparatorBlock](https://raw.githubusercontent.com/mahtomedi/minecraft/main/src/main/java/net/minecraft/world/level/block/ComparatorBlock.java)

The decompiled source mirror has no pinned 1.21 revision; core rules above are
stable Java rules, not a claim of exact 1.21 update-order parity.
