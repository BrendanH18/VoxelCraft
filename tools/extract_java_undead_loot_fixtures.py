#!/usr/bin/env python3
"""Extract supported undead rare pools from the SHA-pinned Java 26.3 server."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import zipfile
from extract_java_item_fixtures import SERVER_SHA1


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError('not the pinned Java 26.3 server')
    with zipfile.ZipFile(io.BytesIO(data)) as bundle:
        inner = bundle.read('META-INF/versions/26.3/server-26.3.jar')
    pools = {}
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:
        for name in ('zombie', 'husk', 'zombie_villager', 'drowned'):
            table = json.loads(jar.read(f'data/minecraft/loot_table/entities/{name}.json'))
            for pool in table['pools']:
                conditions = pool.get('condition', {}).get('terms', [])
                chance = next((c for c in conditions if c['type'] == 'minecraft:random_chance_with_enchanted_bonus'), None)
                if chance is None:
                    continue
                assert any(c['type'] == 'minecraft:killed_by_player' for c in conditions)
                entries = pool['entries']
                assert pool['rolls'] == 1 and all(e.get('weight', 1) == 1 for e in entries)
                pools[name] = {
                    'rolls': pool['rolls'], 'requires_player_kill': True,
                    'chance': chance['unenchanted_chance'],
                    'looting_per_level': chance['enchanted_chance']['per_level_above_first'],
                    'entries': [e['name'].removeprefix('minecraft:') for e in entries],
                    'smelts_potato': any(e['name'] == 'minecraft:potato' and e.get('modifier', {}).get('type') == 'minecraft:furnace_smelt' for e in entries),
                }
    return {'reference': 'Minecraft Java Edition 26.3', 'server_sha1': SERVER_SHA1, 'rare_pools': pools}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('server', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2) + '\n')


if __name__ == '__main__':
    main()
