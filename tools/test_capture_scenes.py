import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import capture_scenes  # noqa: E402


class CaptureScenesTest(unittest.TestCase):
    def setUp(self):
        self.data = capture_scenes.load()

    def scene(self, name):
        return next(s for s in self.data["scenes"] if s["name"] == name)

    def test_world_scene_records_every_setting_on_its_command_line(self):
        scene = self.scene("stone-ore-wall")
        [(suffix, settings)] = capture_scenes.variants(self.data, scene)
        self.assertEqual(suffix, "")
        args = capture_scenes.scene_args(scene, settings)
        joined = " ".join(args)
        for part in [
            "--world capture --new --seed 42 --creative",
            "--size 1920x1080",
            "--gui-scale auto",
            "--time 0.25 --weather clear",
            "--gamerule advance_time=false",
            "--pose 0.5,159.5,12.5,90,0",
            "--no-hud",
            "--animation-time 7.3",
        ]:
            self.assertIn(part, joined)
        # Every filled cell and single placement becomes one --place.
        cells = sum(len(capture_scenes.fill_cells(f)) for f in scene["fill"]) + len(scene["place"])
        self.assertEqual(args.count("--place"), cells)

    def test_fill_is_inclusive_in_any_corner_order(self):
        cells = capture_scenes.fill_cells([1, 5, 0, 0, 5, 1, "stone"])
        self.assertEqual(cells, ["0,5,0,stone", "1,5,0,stone", "0,5,1,stone", "1,5,1,stone"])

    def test_ui_scenes_expand_to_both_resolutions_and_scales(self):
        names = [suffix for suffix, _ in capture_scenes.variants(self.data, self.scene("inventory"))]
        self.assertEqual(
            names,
            ["-1920x1080-guiauto", "-1920x1080-gui2", "-1280x720-guiauto", "-1280x720-gui2"],
        )
        _, settings = capture_scenes.variants(self.data, self.scene("inventory"))[3]
        args = capture_scenes.scene_args(self.scene("inventory"), settings)
        self.assertIn("1280x720", args)
        self.assertEqual(args[args.index("--gui-scale") + 1], "2")

    def test_title_scenes_skip_world_settings(self):
        scene = self.scene("world-list")
        _, settings = capture_scenes.variants(self.data, scene)[0]
        args = capture_scenes.scene_args(scene, settings)
        self.assertNotIn("--world", args)
        self.assertNotIn("--time", args)
        self.assertEqual(args[-2:], ["--open-menu", "select"])

    def test_saves_have_fixed_dates_newest_first(self):
        with tempfile.TemporaryDirectory() as tmp:
            saves = Path(tmp)
            capture_scenes.write_saves(saves, self.scene("world-list")["saves"])
            dirs = sorted(saves.iterdir(), key=lambda p: -os.stat(p / "level.txt").st_mtime)
            self.assertEqual(dirs[0].name, "Survival_Island")
            self.assertEqual(int(os.stat(dirs[0] / "level.txt").st_mtime), capture_scenes.SAVE_EPOCH)
            text = (dirs[0] / "level.txt").read_text()
            self.assertTrue(text.startswith("seed=42\n"), text)
            self.assertIn("name=Survival Island\n", text)

    def test_select_by_name_or_group(self):
        picked = [s["name"] for s in capture_scenes.select(self.data, ["day", "mobs"])]
        self.assertEqual(picked, ["mobs-uneven-ground", "mobs-night", "day"])
        with self.assertRaises(SystemExit):
            capture_scenes.select(self.data, ["nowhere"])

    def test_scene_list_is_valid_json_with_unique_names(self):
        names = [s["name"] for s in self.data["scenes"]]
        self.assertEqual(len(names), len(set(names)))
        json.dumps(self.data)


if __name__ == "__main__":
    unittest.main()
