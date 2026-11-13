# README screenshots

[← Back to VoxelCraft](../../../README.md) · [Screenshots and scripted scenes](../../development.md#screenshots-and-scripted-scenes)

Captured on 2026-11-13 from a local release build of the integration
branch (`ba26920b`) on an Apple M5, at 1600 × 900 with the default
(enhanced) graphics, then saved as JPEG (the hero at 1600 × 900, the rest
at 1280 × 720). Every run also used `--world <name> --new --data-dir
<dir> --size 1600x900 --mute --no-update-check --screenshot <file>`.
These are scripted scenes: mobs are placed with `--spawn`, the
camera with `--pose`.

| Image | Arguments |
| --- | --- |
| `hero.jpg` | `--seed 42 --creative --no-hud --rd 12 --time 0.32 --weather clear --wait 8 --pose -20,150,40,20,-18` |
| `cherry-grove.jpg` | `--seed 42 --creative --no-hud --weather clear --rd 10 --time 0.3 --wait 6 --pose 240,78,368,-45,-32` |
| `desert-village.jpg` | `--seed 7 --creative --no-hud --weather clear --rd 10 --time 0.3 --wait 6 --pose -246,108,-220,-45,-28` |
| `meadow.jpg` | `--seed 42 --creative --no-hud --weather clear --rd 8 --time 0.28 --wait 3 --pose 227,68.5,358,-45,-12`, with `--spawn` horse 236,~,350; sheep 239,~,352; cow 234,~,346; fox 233,~,351; wolf 238,~,347; pig 240,~,349; chicken 235,~,353; rabbit 232,~,349; armadillo 241,~,345; llama 242,~,351 |
| `night.jpg` | as `meadow.jpg` with `--time 0.78`, spawning zombie, skeleton, creeper, spider, enderman, witch and husk at the same points |
| `warden.jpg` | `--seed 42 --creative --no-hud --weather clear --effect night_vision,infinite,0,hideparticles --rd 6 --time 0.3 --wait 3 --spawn warden,146,-48,176 --pose 148,-44,187,-100,-20` |
| `trial-chamber.jpg` | `--seed 42 --creative --no-hud --weather clear --effect night_vision,infinite,0,hideparticles --rd 6 --time 0.3 --wait 2 --spawn breeze,270,-28,100 --pose 265,-26,105,-45,-28` |
| `bastion.jpg` | `--seed 10 --creative --no-hud --weather clear --dimension nether --rd 8 --wait 5 --pose 150,53,33,90,-17` |
| `diamond-armor.jpg` | `--seed 42 --creative --no-hud --weather clear --rd 8 --time 0.3 --wait 3 --camera front --wear diamond_helmet --wear diamond_chestplate --wear diamond_leggings --wear diamond_boots --give diamond_sword --pose 236,65,349,135,8` |
| `sunset.jpg` | `--seed 42 --creative --no-hud --weather clear --rd 12 --time 0.47 --wait 6 --pose 227,72,358,180,-4` |
