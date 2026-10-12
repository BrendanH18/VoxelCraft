> Historical record, superseded on 2026-10-12. Old task instructions, priorities,
> reference versions and completion claims are not current guidance. See
> [the archive index](../../README.md) and [the current roadmap](../../../roadmap.md).

# v0.7 task: LAN hosting and joining (P0A)
Worktree: <assigned-worktree>  Branch: agent/lan
IDs: none (no new blocks/items). If you truly need any, ask in your summary instead.

Goal: two VoxelCraft desktop instances on the same LAN (and over loopback) play in one world, like Java's "Open to LAN". Read roadmap.md sections "P0 — Shared simulation" and "P0A — LAN hosting and joining", docs/architecture.md, docs/agents.md ("LAN and raw protocol"), src/control.rs (existing JSON-lines agent transport), src/agent.rs, src/app/agents.rs, src/app/split.rs and Game::puppet (pad/hosted players already live in the host's simulation with their own inventories, vitals, IDs).

Suggested architecture (adapt if the code suggests better): the host stays the single authority. A joining client is a renderer + input source: it sends movement/look/actions (mine, place, use, attack, inventory/container clicks, chat) each tick; the host simulates that player as a hosted player like pad/agent players, validates actions (reach, edits, containers), and streams to the client: chunks (compressed, nearest-first, per-client subscription around the client's player), block edits in order, entity/mob/item/projectile snapshots with interpolation, the client's own player state (inventory, health/hunger/XP, effects, open container), time/weather, dimension changes, sounds/particles events, other players (model, name, held item, armor). Client predicts its own movement and reconciles with the host. Use a versioned binary protocol with a handshake (protocol version + game version; clear mismatch error), length-prefixed bounded frames, TCP is fine for a first slice (consider separate fast path only if needed). Use local profiles as identity; reconnecting with the same profile restores that player's saved state.

Must have this round:
1. Pause menu "Open to LAN" (game mode + allow cheats like Java), shows the port; host keeps simulating while its menus are open once a client is connected. Title screen "Multiplayer" screen: LAN discovery list (UDP multicast/broadcast announce like Java's 224.0.2.60:4445 "[MOTD]..[/MOTD][AD]port[/AD]" style, own format is fine) plus Direct Connect IP:port. Show connection errors and disconnect reasons.
2. Joining client sees and walks the world, mines/places, uses doors/chests/furnaces/crafting, fights mobs, picks items up, dies/respawns, sleeps (sleep quorum), and travels through portals (if per-dimension worlds for multiple players aren't feasible, document the limitation and do the simplest correct thing, e.g. host-follows or blocked with a message).
3. Player list (Tab), chat (T) with join/leave messages, basic /kick for the host, configurable PvP.
4. Host save-and-quit saves all players and disconnects clients cleanly; clients see "Connection lost"/"Server closed" and return to the title.
5. Headless CLI flags for testing (e.g. `--host-lan`, `--join host:port`) and an automated integration test that runs host+client logic over loopback in-process (no GPU) covering handshake, chunk sync, an edit round-trip, and disconnect. Measure bandwidth and join time and record them in docs/lan.md.
Keep singleplayer behaviour and performance unchanged. Keep split-screen and agents working.
