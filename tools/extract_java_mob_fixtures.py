#!/usr/bin/env python3
"""Pin Java 26.3 mob/environment behavior from the official server artifact."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import zipfile
from extract_java_item_fixtures import SERVER_SHA1


NETHER_BIOMES = ('nether_wastes', 'soul_sand_valley', 'crimson_forest', 'warped_forest', 'basalt_deltas')


def count_range(count):
    if isinstance(count, int):
        return count, count
    return count['min_inclusive'], count['max_inclusive']


def extract(server, scratch):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError('not the pinned Java 26.3 server')
    with zipfile.ZipFile(io.BytesIO(data)) as bundle:
        inner = bundle.read('META-INF/versions/26.3/server-26.3.jar')
    scratch.mkdir(parents=True, exist_ok=True)
    jar_path = scratch / 'inner.jar'
    jar_path.write_bytes(inner)
    def inspect(name):
        return subprocess.check_output(['javap', '-classpath', str(jar_path), '-c', '-p', name], text=True)
    egg = inspect('net.minecraft.world.level.block.DragonEggBlock').split(' private void teleport(')[1].split('\n\n')[0]
    bounds = [int(v) for v in re.findall(r'bipush\s+(\d+)\s*\n\s*\d+: invokeinterface .*RandomSource.nextInt', egg)]
    if len(bounds) != 6 or egg.count('BlockState.isAir:') != 2:
        raise ValueError('egg candidate sampling or support check changed')
    for token in ('WorldBorder.isWithinBounds:', 'Level.isInsideBuildHeight:'):
        if token not in egg:
            raise ValueError('egg bounds check changed')
    attempts = int(re.search(r'sipush\s+(\d+)\s*\n\s*\d+: if_icmpge', egg)[1])
    snow = inspect('net.minecraft.world.entity.animal.golem.SnowGolem')
    if 'EnvironmentAttributes.SNOW_GOLEM_MELTS:' not in snow:
        raise ValueError('snow golem heat check changed')
    biomes = {}
    nether_spawns = {}
    dimension_melts = {}
    with zipfile.ZipFile(io.BytesIO(inner)) as jar:
        for dimension in ('overworld', 'the_nether', 'the_end'):
            attrs = json.loads(jar.read(f'data/minecraft/dimension_type/{dimension}.json')).get('attributes', {})
            dimension_melts[dimension] = attrs.get('minecraft:gameplay/snow_golem_melts', False)
        for name in sorted(jar.namelist()):
            if name.startswith('data/minecraft/worldgen/biome/') and name.endswith('.json'):
                attrs = json.loads(jar.read(name)).get('attributes', {})
                biomes[Path(name).stem] = attrs.get('minecraft:gameplay/snow_golem_melts', False)
                if Path(name).stem in NETHER_BIOMES:
                    spawns = attrs['minecraft:gameplay/natural_mob_spawns']['argument']['spawns_by_category']
                    nether_spawns[Path(name).stem] = {
                        category: [[e['type'].removeprefix('minecraft:'), e['weight'], *count_range(e['count'])]
                                   for e in spawns.get(category, [])]
                        for category in ('monster', 'creature')}
    return {'reference': 'Minecraft Java Edition 26.3', 'server_sha1': SERVER_SHA1,
            'sources': ['DragonEggBlock.teleport', 'SnowGolem.aiStep', 'worldgen/biome/*.json'],
            'nether_spawns': nether_spawns,
            'dragon_egg': {'attempts': attempts, 'draw_bounds_xyz': bounds, 'requires_non_air_below': True},
            'snow_golem_melts': biomes, 'snow_golem_dimension_melts': dimension_melts}


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('server', type=Path)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--scratch', type=Path, default=Path('target/java-reference'))
    args = p.parse_args()
    args.output.write_text(json.dumps(extract(args.server, args.scratch), indent=2) + '\n')


if __name__ == '__main__':
    main()
