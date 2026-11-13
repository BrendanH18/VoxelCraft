# README screenshots

[← Back to VoxelCraft](../../../README.md) · [Screenshots and scripted scenes](../../development.md#screenshots-and-scripted-scenes)

Captured on 2026-11-13 from a local release build of the integration
branch (`ba26920b`) on an Apple M5, at 1600 × 900 with the default
(enhanced) graphics, then saved as JPEG at 1280 × 720 (the hero is cropped to
1600 × 640 from y 40, `the-end.jpg` from (160, 140) and `warden.jpg` from
(0, 90), each without scaling). Every run also used `--world <name> --new --data-dir
<dir> --size 1600x900 --mute --no-update-check --screenshot <file>`.
These are scripted scenes: mobs are placed with `--spawn`, the
camera with `--pose`.

| Image | Arguments |
| --- | --- |
| `hero.jpg` | `--seed 42 --creative --no-hud --rd 12 --time 0.32 --weather clear --wait 8 --pose -20,150,40,20,-18` |
| `desert-village.jpg` | `--seed 7 --creative --no-hud --weather clear --rd 8 --time 0.3 --wait 5 --spawn villager,-208,~,-256 --spawn villager,-205,~,-252 --spawn villager,-211,~,-259 --pose -216,78,-248,-45,-10` |
| `the-end.jpg` | `--seed 42 --creative --no-hud --weather clear --dimension end --rd 8 --wait 7 --pose 45,100,45,-135,-10` (the dragon is the one the End starts with) |
| `meadow.jpg` | `--seed 42 --creative --no-hud --weather clear --rd 8 --time 0.28 --wait 3 --pose 227,68.5,358,-45,-12`, with `--spawn` horse 236,~,350; sheep 239,~,352; cow 234,~,346; fox 233,~,351; wolf 238,~,347; pig 240,~,349; chicken 235,~,353; rabbit 232,~,349; armadillo 241,~,345; llama 242,~,351 (not all are in frame) |
| `night.jpg` | as `meadow.jpg` with `--time 0.78`, spawning zombie, skeleton, creeper, spider, enderman, witch and husk at the first seven points |
| `warden.jpg` | `--seed 42 --creative --no-hud --weather clear --effect night_vision,infinite,0,hideparticles --rd 6 --time 0.3 --wait 3 --spawn warden,147,-48,180 --pose 148,-45,188,-100,-12` |
| `bastion.jpg` | `--seed 10 --creative --no-hud --weather clear --dimension nether --rd 8 --wait 4 --spawn piglin,148,52,40 --spawn piglin,152,52,41 --spawn hoglin,150,52,45 --pose 150,54,33,90,-12` |
| `sunset.jpg` | `--seed 42 --creative --no-hud --weather clear --rd 12 --time 0.47 --wait 6 --pose 227,72,358,180,-4` |
