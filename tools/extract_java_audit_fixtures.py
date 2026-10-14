#!/usr/bin/env python3
"""Reproduce building/plant audit facts from the exact official Java 26.3 server artifact.
JDK javap is required for the item-to-compost-provider associations.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1

COMPOST_NAMES = ['acacia_leaves', 'acacia_sapling', 'allium', 'apple', 'azalea', 'azalea_leaves', 'azure_bluet', 'baked_potato', 'beetroot', 'beetroot_seeds', 'big_dripleaf', 'birch_leaves', 'birch_sapling', 'blue_orchid', 'bread', 'brown_mushroom', 'brown_mushroom_block', 'cactus', 'cake', 'carrot', 'carved_pumpkin', 'cherry_leaves', 'cherry_sapling', 'cocoa_beans', 'cookie', 'cornflower', 'crimson_fungus', 'crimson_roots', 'dandelion', 'dark_oak_leaves', 'dark_oak_sapling', 'dried_kelp', 'dried_kelp_block', 'fern', 'flowering_azalea', 'flowering_azalea_leaves', 'glow_berries', 'hanging_roots', 'hay_block', 'jungle_leaves', 'jungle_sapling', 'kelp', 'large_fern', 'lilac', 'lily_of_the_valley', 'lily_pad', 'mangrove_leaves', 'mangrove_propagule', 'mangrove_roots', 'melon', 'melon_seeds', 'melon_slice', 'moss_block', 'moss_carpet', 'mushroom_stem', 'nether_sprouts', 'nether_wart', 'nether_wart_block', 'oak_sapling', 'orange_tulip', 'oxeye_daisy', 'pale_hanging_moss', 'pale_moss_block', 'pale_moss_carpet', 'pale_oak_leaves', 'pale_oak_sapling', 'peony', 'pink_petals', 'pink_tulip', 'poppy', 'potato', 'pumpkin', 'pumpkin_pie', 'pumpkin_seeds', 'red_mushroom', 'red_mushroom_block', 'red_tulip', 'rose_bush', 'sea_pickle', 'seagrass', 'short_grass', 'shroomlight', 'small_dripleaf', 'spore_blossom', 'spruce_leaves', 'spruce_sapling', 'sugar_cane', 'sunflower', 'sweet_berries', 'tall_grass', 'twisting_vines', 'vine', 'warped_fungus', 'warped_roots', 'warped_wart_block', 'weeping_vines', 'wheat', 'wheat_seeds', 'white_tulip']
BUILDING_NAMES = [f'{wood}_{form}' for wood in ('pale_oak', 'bamboo') for form in ('stairs','slab','fence','fence_gate','door','trapdoor','button','pressure_plate')]
BUILDING_NAMES += ['bamboo_mosaic','bamboo_mosaic_stairs','bamboo_mosaic_slab']
BUILDING_NAMES += [f'{stone}_{form}' for stone in ('mud_brick','prismarine','prismarine_brick','dark_prismarine') for form in ('stairs','slab')]+['mud_brick_wall','prismarine_wall']

def associations(jar_path):
    # Some colored collections and property builders delegate through another callback.
    from java_item_associations import provider_associations
    return provider_associations(jar_path, "COMPOSTABLE_")

def extract(server):
    data=server.read_bytes()
    if hashlib.sha1(data).hexdigest()!=SERVER_SHA1: raise ValueError('not the pinned official Java 26.3 server artifact')
    with zipfile.ZipFile(io.BytesIO(data)) as bundler: data=bundler.read('META-INF/versions/26.3/server-26.3.jar')
    with tempfile.TemporaryDirectory(prefix='voxelcraft-java-fixtures-') as temporary:
        path=Path(temporary)/'reference.jar';path.write_bytes(data);compost_providers=associations(path)
    with zipfile.ZipFile(io.BytesIO(data)) as jar:
        def read(path): return json.loads(jar.read(path))
        recipes=[]
        for name in BUILDING_NAMES:
            path=f'data/minecraft/recipe/{name}.json';recipe=read(path)
            if 'pattern' in recipe: pattern,key=recipe['pattern'],recipe['key']
            else:
                if len(recipe['ingredients'])!=1: raise ValueError(f'{path}: update shapeless setup normalization')
                pattern,key=['#'],{'#':recipe['ingredients'][0]}
            recipes.append({'path':path,'type':recipe['type'],'pattern':pattern,'ingredients':key,'result':recipe['result']['id'],'count':recipe['result'].get('count',1)})
        building={'reference':'Minecraft Java Edition 26.3','server_sha1':SERVER_SHA1,'recipes':recipes,'intentionally_absent':['bamboo_mosaic_fence','bamboo_mosaic_door','prismarine_brick_wall','dark_prismarine_wall']}
        compost=[]
        for name in COMPOST_NAMES:
            provider=read(f'data/minecraft/context_int_provider/compostable/{compost_providers[name]}.json')
            if provider==1: percent=100
            else:
                # The default branch is the ordinary, non-empty composter roll.
                weighted=provider['default']['distribution']
                percent=round(sum(entry['weight']*entry['data'] for entry in weighted)/sum(entry['weight'] for entry in weighted)*100)
            compost.append({'item':'minecraft:'+name,'chance_percent':percent})
        fish=read('data/minecraft/loot_table/gameplay/fishing/fish.json')
        fish_cases=[{'item':entry['name'],'weight':entry.get('weight',1)} for entry in fish['pools'][0]['entries']]
        cocoa=read('data/minecraft/loot_table/blocks/cocoa.json')
        # Explicitly assert the reviewed loot form rather than silently ignoring a changed reference.
        raw=json.dumps(cocoa)
        if 'minecraft:apply_bonus' in raw or 'minecraft:fortune' in raw: raise ValueError('cocoa now has an enchantment bonus; reassess the fixture')
        if read('data/minecraft/loot_table/blocks/budding_amethyst.json').get('pools',[]): raise ValueError('budding amethyst loot changed; reassess')
        plants={'reference':'Minecraft Java Edition 26.3','server_sha1':SERVER_SHA1,'sources':['net.minecraft.world.item.Items compostable registrations','data/minecraft/context_int_provider/compostable/*.json','data/minecraft/loot_table/blocks/cocoa.json','data/minecraft/loot_table/blocks/budding_amethyst.json','data/minecraft/loot_table/gameplay/fishing/fish.json'],'compost':compost,'cocoa':{'unripe':1,'ripe':3,'fortune_bonus':0},'budding_amethyst':{'ordinary':0,'silk_touch':0},'fish':fish_cases}
    return building,plants

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('server',type=Path);parser.add_argument('--output-dir',type=Path,required=True);args=parser.parse_args()
    building,plants=extract(args.server);args.output_dir.mkdir(parents=True,exist_ok=True)
    for name,value in [('building_integrations',building),('plant_integrations',plants)]: (args.output_dir/f'{name}.json').write_text(json.dumps(value,indent=2)+'\n')
if __name__=='__main__':main()
