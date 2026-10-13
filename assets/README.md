# Original resource assets

`manifest.json` is the authoritative map from stable `voxelcraft:` keys to files,
provenance, licenses and runtime resources. These resources are original and
covered by the project's MIT OR Apache-2.0 license. The stone's palette and pixel
layout and the pickup cue's composition parameters are tracked beside them.
No Minecraft textures, recordings, melodies or model files are included.

The build script embeds every declared resource and the engine embeds the
manifest. Missing files and duplicate keys fail the build with the asset key.
The installed executable does not read the checkout or its current directory.
Unmapped resources continue using the existing Rust procedural generators.

Supported formats are 16 x 16 RGBA8 PNG textures, schema-1 static box models
(up to ten boxes in sixteenths, with a texture key), and mono PCM16 WAV at
48 kHz. Entity animations and the original background compositions retain their
existing Rust implementations. Add format/schema and runtime support together
before introducing an unsupported animation or file kind.

Run `python3 tools/validate_assets.py`. Add `--executable /absolute/path/voxelcraft`
to validate a development or packaged binary from a temporary directory outside
the checkout. `voxelcraft --validate-assets` reports embedded bytes and validation
time without opening a window, GPU or audio device. Inspect texture/model assets
in ordinary gameplay and listen to audio before marking their presentation
requirements Verified. Format checks alone do not establish aesthetic quality.
