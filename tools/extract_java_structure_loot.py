#!/usr/bin/env python3
"""Normalize Java 26.3 loot algorithms' numeric inputs; copies no artwork."""
import argparse
import json
from pathlib import Path

TABLES = ['bastion_treasure', 'bastion_bridge', 'bastion_hoglin_stable', 'bastion_other',
          'desert_pyramid', 'jungle_temple', 'jungle_temple_dispenser', 'igloo_chest',
          'pillager_outpost', 'shipwreck_supply', 'shipwreck_treasure', 'shipwreck_map',
          'underwater_ruin_small', 'underwater_ruin_big', 'ruined_portal', 'buried_treasure', 'nether_bridge',
          'abandoned_mineshaft', 'simple_dungeon', 'stronghold_corridor',
          'stronghold_crossing', 'stronghold_library', 'ancient_city', 'ancient_city_ice_box',
          'village/village_cartographer',
          *['village/village_' + s + '_house' for s in ['plains', 'desert', 'savanna', 'taiga', 'snowy']]]


def extract(base):
    def read(p):
        return json.loads((base / (p + '.json')).read_text())

    def expand(kind, names):
        if isinstance(names, str):
            names = [names]
        out = []
        for name in names:
            if name.startswith('#minecraft:'):
                values = read('tags/' + kind + '/' + name.removeprefix('#minecraft:'))['values']
                values = expand(kind, values)
            else:
                values = [name.removeprefix('minecraft:')]
            for value in values:
                if value not in out:
                    out.append(value)
        return out

    enchantments = {}
    order = expand('enchantment', '#minecraft:on_random_loot')
    for file in sorted((base / "enchantment").glob("*.json")):
        name = file.stem
        e = read('enchantment/' + name)
        enchantments[name] = {k: e[k] for k in ['weight', 'max_level', 'min_cost', 'max_cost']}
        enchantments[name]['supported'] = expand('item', e['supported_items'])
        enchantments[name]['primary'] = expand('item', e.get('primary_items', e['supported_items']))
        enchantments[name]['exclusive'] = expand('enchantment', e.get('exclusive_set', []))
    return {'reference': 'Java 26.3 behaviour data only; no artwork or structure templates',
            'tables': {name: read('loot_table/chests/' + name) for name in TABLES},
            'enchantment_order': order, 'enchantments': enchantments}


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('data', type=Path)
    p.add_argument('output', type=Path)
    a = p.parse_args()
    a.output.write_text(json.dumps(extract(a.data), separators=(',', ':')) + '\n')
