# Procedural music

VoxelCraft renders original music in Rust. No recordings, sound fonts,
Minecraft melodies or external audio assets are used. The desktop client
owns one music stream, including during split-screen and on the title screen.

The composer offers three harmonic forms per situation. SplitMix-seeded
choices vary key, tempo, dynamics, inversions, phrase omissions and answers.
A recurring diatonic motif and an A/B/A texture give each piece a beginning,
contrast and return. Pad voices choose nearby chord tones without crossing;
soft piano partials lose brightness as they decay. Underwater/End pieces are
sparser and pad-heavy, Nether pieces use a lower minor register, creative and
credits have fuller piano phrases, and dragon music has a quicker pulse.

Synthesis uses a 24 kHz stereo stream, interpolated wavetables, 24 fixed voice
slots, cubic attacks/releases, damped combs and allpass stereo diffusion,
DC blocking and gentle track fades. The sound-effect limiter remains the
final master-bus protection; music scores retain substantial headroom without
normalizing every quiet passage. A background thread renders at most 512
frames at a time into a 4096-frame atomic ring (171 ms). The callback only
reads/interpolates stereo PCM and ramps the Music category. It allocates no
memory, takes no locks, and does no composition or reverb work. An underrun
decays smoothly and recovers over 10 ms. Dropping the reader stops the worker,
including when audio-device creation fails.

## Java rules

Reference baseline is Java 1.21.4, including the newer MusicInfo fade system.
The delay clock is 20 Hz and runs independently of simulation pause:

| Situation | Min/max ticks after a track | Replace another situation |
| --- | --- | --- |
| Menu | 20 / 600 | yes |
| Game, creative, underwater, biome music | 12000 / 24000 | no |
| End | 6000 / 24000 | yes |
| Dragon, credits | 0 / 0 | yes |

The initial delay is 100 ticks. On replacement, the delay is sampled from
0 through half the new minimum. Completion samples the new situation's
inclusive range. Outstanding delays are capped to the new maximum, even
while a track is playing. Selection gives screen overrides priority, then
End/dragon, ocean-only submerged music, creative outside the Nether, and
biome music/game fallback. Underwater selection stays latched until that
track ends after surfacing; dimension or screen overrides still win.

MusicInfo situational volume falls 3% towards its target each tick, snapping
within 0.0001, and increases by clamp(current, 0.0005, 0.005) per tick. Reaching
silence stops playback and adds 100 ticks to the delay. A null selection keeps
a minimum 100-tick delay without starting tracks. The renderer interpolates
these tick gains over PCM frames. Separately, the saved `music_volume` option
and Music slider multiply music by master volume, without changing effects
(every other sound category has its own slider too; see
[gameplay](gameplay.md#sound-categories)).
Music is non-positional and bypasses underwater environmental muffling.

Intentional adaptation: Java stops a replaced track immediately; VoxelCraft
retains a one-second decaying tail to prevent a waveform discontinuity.
The next-track scheduling rules are unchanged. Music is generated, so Java's
licensed track catalog and weighted sound-event playlists are not reproduced.
Available Overworld biomes select corresponding procedural palettes.

## Integration hooks and remaining work

`voxelcraft::music` is available with `--no-default-features`. `Context` and
`Manager` can be tested without a window, device or output thread.
`MusicStream::set_context`, `set_situation`, and `set_info` are the integration
points. The desktop `Audio::update_music` chooses the primary player's biome,
creative status, immersion and dragon boss bar each frame. `leave_world`
returns to menu music. No extra stream is created for additional players.

The credits screen (`src/app/credits.rs`), shown on the first trip through the
End exit portal, passes `credits` to `Audio::update_music`, which selects
`Situation::Credits` on the shared stream and returns to normal selection when
the screen closes. In the Nether, `Generator::nether_biome` at the player's column picks the
Nether Wastes, Crimson Forest, Warped Forest, Soul Sand Valley or Basalt
Deltas palette. Silent biomes can call `set_info(MusicInfo { music: None, volume: 0.0 })`.

`Audio::update_music` also sets the situational volume from Java 26.3's
`EnvironmentAttributes.MUSIC_VOLUME` (`Minecraft.getMusicVolume`): 0 in
the pale garden, so the current track fades out there and none plays, and 1
elsewhere ([v1.0 final F4](verification/2026-11-28-v1.0-final-f4-sounds.md)).
Java fades no music while a jukebox plays: `LevelEventHandler.playJukeboxSong`
only starts the song, and nothing but that attribute (or a screen with menu
music) moves `MusicManager`'s gain.

## Verification

`cargo run --release -- --export-music --seed 42` renders nine full-length,
24 kHz stereo PCM WAVs to `target/music`. It prints per-channel peak, RMS,
DC, maximum adjacent-sample delta, boundary amplitude, four-band spectrum,
composition setup time and synthesis CPU/wall-time ratio. It rejects unsafe
levels, DC, discontinuities, boundary clicks and excessive high-band energy.
Synthesis timing excludes WAV encoding, spectral analysis and file I/O.
Tests cover all three forms for every situation over complete tracks, seeded
repeatability across render block sizes, selection priority, delays,
replacement, MusicInfo silence/fades and ring wrap/underrun behavior.

Numerical analysis cannot establish hour-long subjective listening quality;
listening review of the exported samples is still appropriate.

## Sources

- [Minecraft Wiki: Music](https://minecraft.wiki/w/Music) (wiki fetch was blocked;
  exact implementation rules below were checked against client source).
- [Java MusicManager](https://github.com/KernelDotDLL/mcp-1.21.4/blob/master/src/main/java/net/minecraft/client/sounds/MusicManager.java)
- [Java Musics constants](https://github.com/KernelDotDLL/mcp-1.21.4/blob/master/src/main/java/net/minecraft/sounds/Musics.java)
- [Java MusicInfo](https://github.com/KernelDotDLL/mcp-1.21.4/blob/master/src/main/java/net/minecraft/client/sounds/MusicInfo.java)
- [Java situational selection](https://github.com/KernelDotDLL/mcp-1.21.4/blob/master/src/main/java/net/minecraft/client/Minecraft.java)
- [Java Nether biome music](https://github.com/KernelDotDLL/mcp-1.21.4/blob/master/src/main/java/net/minecraft/data/worldgen/biome/NetherBiomes.java)

## Block sounds, subtitles and container lids (v0.18 Track 9)

Sound effects are synthesized in `src/audio` like the music. Since v0.18
every block plays its Java 26.3 `SoundType`: `tests/fixtures/java/26.3/sounds.json`
(written by `tools/extract_java_sound_fixtures.py` from the pinned jars and
the client's `sounds.json` asset) names each Java block's type, and each
type's volume, pitch and break, step, place, hit and fall events. A state
finds its Java block by name, so no state's sound is guessed; the few
VoxelCraft-only states keep the older shared materials. Each Java event
family (`block.stone.*`, `block.deepslate.*`, `block.cherry_wood.*`, …)
has its own original recipe in `src/audio/block_sounds.rs`, and a family's
hit and fall reuse its step buffers, as Java's events reuse the step files.
Volumes and pitches follow Java's call sites (breaking and placing at
`(volume + 1) / 2` and `pitch * 0.8`, steps at `volume * 0.15`, mining hits
every fourth tick at `(volume + 1) / 8` and `pitch * 0.5`, falls at
`volume * 0.5` and `pitch * 0.75`), scaled once so that a volume-1 type is as
loud as before.

Closed Captions (Java's `showSubtitles`, off by default) shows Java's
subtitle overlay: each played sound's Java event names its English caption,
repeats of the same text merge, captions fade from white to grey over three
seconds, and `<` or `>` points to a sound more than 60 degrees off the view.
The captions stack up from the bottom-right corner on Text Background's
backing (`backgroundForChatOnly`: Chat keeps the overlay's 80% black;
Everywhere uses Text Background Opacity). `src/audio/events.rs` maps every
sound to its Java event; the verification record lists stand-ins and the
Java subtitles the game never plays.

Chests, trapped and copper chests and ender chests open their lids at
`ChestLidController`'s 0.1 a tick (eased as `ChestRenderer` eases them),
and barrels show their open top, while anyone (a player on any input path,
a LAN guest or a copper golem) has them open. The first viewer's arrival
and the last one's leaving play the container's open and close sounds.
Only containers in use or still moving are drawn as models; the chunk mesh
leaves them out and is rebuilt once when one enters or leaves.

## Goat horns, brushing, discs and mobs' own voices (v1.0 final F4)

[Record](verification/2026-11-28-v1.0-final-f4-sounds.md), implemented,
unverified; nobody has listened to these yet.

- **Goat horns** play Java's eight instruments (`item.goat_horn.sound.0`
  to `.7`, `src/audio/horns.rs`): original brassy calls, each with its own
  melody, breath and a valley echo, in `RECORDS` and heard linearly to 256
  blocks (`InstrumentItem.play`'s volume 16).
- **Discs**: a jukebox's tune plays through the note voices as
  `Sound::Disc`, heard linearly to 64 blocks (`forJukeboxSong`'s volume 4).
  Only these two sounds use Java's linear `16 * volume` range
  (`Sound::java_range`); every other sound keeps the mixer's 2 to 24 block
  fade.
- **Brushing** plays `item.brush.brushing.sand`, `.gravel` or `.generic`
  each stroke (`BLOCKS`) and `.sand.complete` or `.gravel.complete` when
  the find comes free (`PLAYERS`).
- **Mobs' own voices** (`src/audio/creatures.rs`): iron golems (no ambient
  call, as in Java), llamas, camels, snow golems, phantoms, silverfish,
  turtles and baby turtles have their own recipes; husks, drowned, zombie
  villagers, zombified piglins, strays, bogged, wither skeletons, magma
  cubes, wandering traders, skeleton and zombie horses, donkeys, mules,
  piglin brutes, zoglins, glow squid and elder guardians take their kin's
  recipe transposed, filtered and layered with a texture of their own.
  Iron golems, llamas, camels (with a sand step), silverfish and turtles
  play their own footsteps every 1.67 blocks, as Java's `Entity.move`.
  Fish still share one voice.
