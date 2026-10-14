#!/usr/bin/env python3
"""Extract bounded v0.7 world-defect facts from the pinned Java 26.3 artifact.

Data facts only: spawn costs, structure loot weights/counts/modifier names.
No engine source or existing fixtures are consulted. Missing content is not
substituted. RNG sequences and enchantment candidate ordering are out of scope.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import zipfile
from extract_java_item_fixtures import SERVER_SHA1

TABLES = ('desert_pyramid', 'jungle_temple', 'igloo_chest', 'pillager_outpost',
          'shipwreck_supply', 'shipwreck_treasure', 'shipwreck_map',
          'underwater_ruin_small', 'underwater_ruin_big')


def bounds(value):
    if isinstance(value, (int, float)):
        return [value, value]
    assert value['type'] == 'minecraft:uniform', value
    return [value['min'], value['max']]


def extract(data):
    read = lambda name: json.loads(data.read('data/minecraft/' + name + '.json'))
    costs = {}
    for biome in ('nether_wastes', 'soul_sand_valley', 'crimson_forest', 'warped_forest', 'basalt_deltas'):
        costs[biome] = read('worldgen/biome/' + biome)['attributes']['minecraft:gameplay/natural_mob_spawns']['argument'].get('spawn_costs', {})
    tables = {}
    for name in TABLES:
        pools = []
        for pool in read('loot_table/chests/' + name)['pools']:
            entries = []
            for entry in pool['entries']:
                mods = entry.get('modifier', [])
                if isinstance(mods, dict):
                    mods = [mods]
                count = next((bounds(m['count']) for m in mods if m['type'] == 'minecraft:set_count'), [1, 1])
                entries.append({'name': entry.get('name'), 'weight': entry.get('weight', 1),
                                'count': count, 'modifiers': [m['type'] for m in mods]})
            pools.append({'rolls': bounds(pool['rolls']), 'entries': entries,
                          'modifier': pool.get('modifier')})
        tables[name] = pools
    return {'reference': 'Java 26.3', 'server_sha1': SERVER_SHA1, 'spawn_costs': costs, 'structure_loot': tables,
            'regular_goat_horns': read('tags/instrument/regular_goat_horns')['values']}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('server', type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    outer = args.server.read_bytes()
    assert hashlib.sha1(outer).hexdigest() == SERVER_SHA1, 'not the pinned server'
    with zipfile.ZipFile(io.BytesIO(outer)) as bundler:
        inner = bundler.read('META-INF/versions/26.3/server-26.3.jar')
    with zipfile.ZipFile(io.BytesIO(inner)) as data:
        result = extract(data)
    args.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
