#!/usr/bin/env python3
"""Extract map-colour behaviour (no artwork) from the Java server classes."""
import argparse
import json
import re
from pathlib import Path
from java_block_properties import block_map_colors

p = argparse.ArgumentParser()
p.add_argument('jar', type=Path)
p.add_argument('map_color_source', type=Path)
p.add_argument('output', type=Path)
a = p.parse_args()
colors = {name: int(i) for name, i in re.findall(r'public static final MapColor (\w+) = new MapColor\((\d+),', a.map_color_source.read_text())}
blocks = block_map_colors(a.jar)
def normalize(value):
    if isinstance(value, dict):
        if "axis" in value:
            return {"axis": [colors.get(c) for c in value["axis"]]}
        if "bed" in value:
            name = value["bed"]
            name = "SNOW" if name == "WHITE" else "COLOR_" + ("LIGHT_GREEN" if name == "LIME" else name)
            return {"bed": colors[name]}
        if "waterlogged" in value:
            return {"waterlogged": colors[value["waterlogged"]]}
    return colors.get(value)
a.output.write_text(json.dumps({'reference': 'Java 26.3 Blocks properties, symbolic javap evaluation; state_dependent is unresolved', 'blocks': {k: normalize(v) for k, v in sorted(blocks.items())}}, indent=2)+'\n')
