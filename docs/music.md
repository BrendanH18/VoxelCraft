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
and Music slider multiply music by master volume, without changing effects.
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
the screen closes. The Nether generator has no biome identity API yet, so
Nether Wastes is the active Nether palette; Crimson Forest, Warped Forest, Soul
Sand Valley and Basalt Deltas are implemented and tested selections awaiting
that API. Silent biomes can call `set_info(MusicInfo { music: None, volume: 0.0 })`.

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
