#!/usr/bin/env python3
"""Extract Java 26.3's fog, sky, cloud, rain and sky-flash rules (v0.18 Track 8).

Reads the pinned client jar: the environment attribute defaults
(`EnvironmentAttributes`), the dimension types', biomes' and day timeline's
visual attributes (`data/minecraft/...`), the weather layers
(`WeatherAttributes`), `FogRenderer` and its environments, the fog, sky and
cloud shaders, `CloudRenderer`, `WeatherEffectRenderer`, the water vision
curve (`LocalPlayer`), the lightning sky flash (`ClientLevel`,
`LightningBolt`) and the Clouds, Cloud Distance and Hide Sky Flashes
options with their English labels. Numbers come from bytecode with `javap`;
each method's literal sequence is checked against the expected one, so a
changed jar fails loudly instead of writing a wrong fixture.

Usage: extract_java_atmosphere_fixtures.py client.jar --output
tests/fixtures/java/26.3/atmosphere.json (`javap` must be on PATH).
"""

import argparse
import hashlib
import json
import re
import zipfile
from pathlib import Path

from extract_java_minecart_fixtures import expect, javap, literals, method
from extract_java_registry_names import CLIENT_SHA1

FOG = "net.minecraft.client.renderer.fog."
ENV = FOG + "environment."
VISUAL = ("sky_color", "fog_color", "water_fog_color", "water_fog_end_distance", "cloud_color", "cloud_height")


def rgb(value):
    """A Java RGB24/ARGB32 int as `#rrggbb` (or `#aarrggbb` when `alpha`)."""
    return f"#{int(value) & 0xFFFFFF:06x}"


def argb(value):
    return f"#{int(value) & 0xFFFFFFFF:08x}"


def attribute_defaults(jar):
    """`visual/*` attribute defaults from `EnvironmentAttributes.<clinit>`."""
    code = method(javap(jar, "net.minecraft.world.attribute.EnvironmentAttributes"), "static {}")
    out = {}
    for name, body in re.findall(r"// String visual/(\w+)\n(.*?)(?=// String |\Z)", code, re.S):
        values = literals(body)
        if values:
            out[name] = values[0]
    want = {
        "fog_start_distance": 0.0,
        "fog_end_distance": 1024.0,
        "sky_fog_end_distance": 512.0,
        "cloud_fog_end_distance": 2048.0,
        "water_fog_start_distance": -8.0,
        "water_fog_end_distance": 96.0,
    }
    for key, value in want.items():
        expect(f"default {key}", out[key], value)
    return {
        **want,
        "cloud_height": round(expect("default cloud_height", out["cloud_height"], 192.3300018310547), 2),
        "water_fog_color": rgb(expect("default water fog", out["water_fog_color"], -16448205.0)),
        "fog_color": "#000000",
        "sky_color": "#000000",
        "cloud_color": "#00000000",
        "block_light_tint": rgb(expect("default block light tint", out["block_light_tint"], -10100)),
        "ambient_light_color": rgb(out["ambient_light_color"]),
    }


def visual_attributes(attributes):
    """The visual attributes VoxelCraft's atmosphere reads from a data file's `attributes`."""
    out = {}
    for key in VISUAL:
        value = attributes.get(f"minecraft:visual/{key}")
        if isinstance(value, dict):
            expect(f"{key} modifier", value["modifier"], "multiply")
            out[f"{key}_multiplier"] = value["argument"]
        elif value is not None:
            out[key] = value
    return out


def dimensions(archive):
    out = {}
    for name in ("overworld", "the_nether", "the_end"):
        data = json.loads(archive.read(f"data/minecraft/dimension_type/{name}.json"))
        attributes = data["attributes"]
        entry = {
            "has_skylight": data["has_skylight"],
            "has_ceiling": data["has_ceiling"],
            "min_y": data["min_y"],
            "timelines": data["timelines"],
            **visual_attributes(attributes),
        }
        for key in ("fog_start_distance", "fog_end_distance", "ambient_light_color"):
            value = attributes.get(f"minecraft:visual/{key}")
            if value is not None:
                entry[key] = value
        out[name] = entry
    return out


def biomes(archive):
    out = {}
    prefix = "data/minecraft/worldgen/biome/"
    for name in sorted(n for n in archive.namelist() if n.startswith(prefix) and n.endswith(".json")):
        data = json.loads(archive.read(name))
        out[name[len(prefix) : -5]] = {
            "has_precipitation": data["has_precipitation"],
            **visual_attributes(data.get("attributes", {})),
        }
    return out


def day_timeline(archive):
    data = json.loads(archive.read("data/minecraft/timeline/day.json"))
    tracks = {}
    for key in ("fog_color", "sky_color", "cloud_color", "sunrise_sunset_color", "sun_angle"):
        track = data["tracks"][f"minecraft:visual/{key}"]
        tracks[key] = {
            "modifier": track.get("modifier", "override"),
            "ease": track.get("ease", "linear"),
            "keyframes": [[k["ticks"], k["value"]] for k in track["keyframes"]],
        }
    return {"period_ticks": data["period_ticks"], "tracks": tracks}


def weather(jar):
    """`WeatherAttributes`: rain and thunder layers for the sky, fog and cloud colours."""
    values = literals(method(javap(jar, "net.minecraft.world.attribute.WeatherAttributes"), "static {}"))
    rain, thunder = values[:18], values[18:]
    expect(
        "rain colour modifiers",
        rain[:8],
        [0.6000000238418579, 0.75, 1.0, 0.5, 0.5, 0.6000000238418579, 0.23999999463558197, 0.5],
    )
    expect(
        "thunder colour modifiers",
        thunder[1:9],
        [
            0.23999999463558197,
            0.9399999976158142,
            1.0,
            0.25,
            0.25,
            0.30000001192092896,
            0.0949999988079071,
            0.9399999976158142,
        ],
    )

    def layer(v):
        return {
            "sky_color_blend_to_gray": {"brightness": v[0], "factor": v[1]},
            "fog_color_multiply": v[3:6],
            "cloud_color_blend_to_gray": {"brightness": v[6], "factor": v[7]},
        }

    return {
        "order": "rain at (rain level - thunder level), then thunder at the thunder level; each lerps from the value to the modified value",
        "greyscale_weights": [0.3, 0.59, 0.11],
        "rain": layer(rain),
        "thunder": layer(thunder[1:]),
    }


def fog(jar):
    renderer = javap(jar, FOG + "FogRenderer")
    setup = expect("FogRenderer.setupFog", literals(method(renderer, "setupFog(")), [0, 16, 10.0, 4.0, 64.0])
    color = literals(method(renderer, "computeFogColor("))
    expect("boss darkening", color[5:8], [0.699999988079071, 0.6000000238418579, 0.6000000238418579])
    order = re.findall(
        r"new\s+#\d+\s+// class net/minecraft/client/renderer/fog/environment/(\w+)", method(renderer, "static {}")
    )
    atmospheric = javap(jar, ENV + "AtmosphericFogEnvironment")
    base = expect(
        "atmospheric base colour",
        literals(method(atmospheric, "getBaseColor(")),
        [4, 0.01745329238474369, 0.0, -1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 16.0, 32.0, 0.25, 1.0, 1.0, 0.25],
    )
    rain = expect(
        "atmospheric setup", literals(method(atmospheric, "setupFog(")), [0, -160.0, 96.0, -256.0, 16, 10.0, 96.0]
    )
    state = expect(
        "rain fog state",
        literals(method(atmospheric, "updateRainFogState(")),
        [0, 8.0, 7.0, 0.0, 1.0, 1.0, 0.5, 0.20000000298023224],
    )
    lava = expect(
        "lava fog",
        literals(method(javap(jar, ENV + "LavaFogEnvironment"), "setupFog(")),
        [-8.0, 0.5, 0.0, 5.0, 0.25, 1.0],
    )
    lava_color = literals(method(javap(jar, ENV + "LavaFogEnvironment"), "static {}"))[0]
    snow = expect(
        "powder snow fog",
        literals(method(javap(jar, ENV + "PowderedSnowFogEnvironment"), "setupFog(")),
        [-8.0, 0.5, 0.0, 2.0],
    )
    snow_color = literals(method(javap(jar, ENV + "PowderedSnowFogEnvironment"), "static {}"))[0]
    blind = expect(
        "blindness fog",
        literals(method(javap(jar, ENV + "BlindnessFogEnvironment"), "setupFog(")),
        [5.0, 1.0, 20.0, 5.0, 0.25, 0.800000011920929, 0.800000011920929],
    )
    dark = expect(
        "darkness fog", literals(method(javap(jar, ENV + "DarknessFogEnvironment"), "setupFog(")), [0, 15.0, 0.75]
    )
    water = expect("water fog", literals(method(javap(jar, ENV + "WaterFogEnvironment"), "setupFog(")), [0, 0.25])
    vision = expect(
        "water vision",
        literals(method(javap(jar, "net.minecraft.client.player.LocalPlayer"), "getWaterVision(")),
        [
            0.0,
            600.0,
            100.0,
            600.0,
            1.0,
            100.0,
            0.0,
            1.0,
            100.0,
            0.0,
            100.0,
            500.0,
            0.0,
            1.0,
            0.6000000238418579,
            0.3999999761581421,
        ],
    )
    return {
        "environment_order": order,
        "render_distance": {
            "blocks_per_chunk": setup[1],
            "span_divisor": setup[2],
            "span_min": setup[3],
            "span_max": setup[4],
            "rule": "start = blocks - clamp(blocks / 10, 4, 64), end = blocks, on cylindrical distance",
        },
        "atmospheric": {
            "sunrise_min_render_distance_chunks": base[0],
            "sky_mix_chunks": base[10],
            "sky_mix_min": base[11],
            "sky_mix_power": base[14],
            "sky_mix_rule": "1 - lerp(clamp(min(sky_fog_end / 16, chunks) / 32), 0.25, 1) ^ 0.25 toward the sky colour",
            "rain_start_offset": rain[1],
            "rain_end_offset": rain[3],
            "rain_min_end": rain[2],
            "rain_sky_light_min": state[1],
            "rain_sky_light_span": state[2],
            "no_precipitation_factor": state[6],
            "rain_fog_rate_per_tick": state[7],
        },
        "lava": {
            "color": rgb(lava_color),
            "start": lava[4],
            "end": lava[5],
            "fire_resistance_start": lava[2],
            "fire_resistance_end": lava[3],
            "spectator_start": lava[0],
            "spectator_end_fraction": lava[1],
        },
        "powder_snow": {
            "color": rgb(snow_color),
            "start": snow[2],
            "end": snow[3],
            "spectator_start": snow[0],
            "spectator_end_fraction": snow[1],
        },
        "blindness": {
            "distance": blind[0],
            "fade_ticks": blind[2],
            "start_fraction": blind[4],
            "sky_cloud_fraction": blind[5],
        },
        "darkness": {"distance": dark[1], "start_fraction": dark[2]},
        "water": {"vision_min_end_fraction": water[1]},
        "water_vision": {
            "max_ticks": vision[1],
            "mid_ticks": vision[2],
            "early_weight": vision[14],
            "late_weight": vision[15],
            "rule": "min(t/100, 1) * 0.6 + clamp((t - 100) / 500, 0, 1) * 0.4 under water, 1 from 600 ticks; +1 per tick under water, -10 out",
        },
        "boss_darkening": color[5:8],
    }


def shaders(archive):
    """Check the fog, sky and cloud shaders' formulas and read the cloud face shades."""
    fog_glsl = archive.read("assets/minecraft/shaders/include/fog.glsl").decode()
    for text in (
        "return max(linear_fog_value(sphericalVertexDistance, environmentalStart, environmantalEnd), "
        "linear_fog_value(cylindricalVertexDistance, renderDistanceStart, renderDistanceEnd));",
        "float distXZ = length(pos.xz);",
        "return max(distXZ, distY);",
        "return (vertexDistance - fogStart) / (fogEnd - fogStart);",
    ):
        expect("fog.glsl", text in fog_glsl, True)
    sky = archive.read("assets/minecraft/shaders/core/sky.fsh").decode()
    expect("sky.fsh", "0.0, FogSkyEnd, FogSkyEnd, FogSkyEnd" in sky, True)
    clouds_v = archive.read("assets/minecraft/shaders/core/clouds.vsh").decode()
    clouds_f = archive.read("assets/minecraft/shaders/core/clouds.fsh").decode()
    expect("clouds.fsh", "color.a *= 1.0f - linear_fog_value(vertexDistance, 0, FogCloudsEnd);" in clouds_f, True)
    block = re.search(r"faceColors = vec4\[\]\((.*?)\);", clouds_v, re.S)[1]
    shades = [float(v) for v in re.findall(r"vec4\(([\d.]+),", block)]
    return {
        "fog": "max(linear(spherical distance, environmental start, end), linear(cylindrical distance, render-distance start, end)); cylindrical = max(length(xz), |y|)",
        "sky": "fog value = linear(spherical distance to the sky plane, 0, sky end), mixed toward the fog colour",
        "cloud_alpha": "alpha * (1 - linear(spherical distance, 0, cloud end))",
        "cloud_face_shades": dict(zip(["down", "up", "north", "south", "west", "east"], shades)),
    }


def clouds(jar):
    renderer = javap(jar, "net.minecraft.client.renderer.CloudRenderer")
    prepare = literals(method(renderer, "public void prepare("))
    expect(
        "cloud prepare", prepare[:9], [16, 12.0, 258, 4.0, 0.0, 0.0, 400.0, 0.030000001192092896, 3.9600000381469727]
    )
    flat = "bipush        32" in method(renderer, "buildFlatCell(")
    return {
        "cell_size_blocks": prepare[1],
        "thickness_blocks": prepare[3],
        "drift_blocks_per_tick": prepare[7],
        "z_offset_blocks": prepare[8],
        "ticks_per_cell": prepare[6],
        "radius_rule": "ceil(Cloud Distance chunks * 16 / 12) cells, a disc of cells",
        "fast": "one downward face per filled cell at the cloud base, shaded with the top face's colour",
        "fancy": "box cells 12 x 4 x 12: top unless the camera is below the base, bottom unless above the top, sides only toward the camera where the neighbour is empty; inside faces for the 3 x 3 cells around the camera",
        "flat_uses_top_colour": expect("flat clouds use the top colour", flat, True),
    }


def rain(jar):
    values = literals(method(javap(jar, "net.minecraft.client.renderer.WeatherEffectRenderer"), "prepareInstances("))
    expect("rain alpha", values[:4], [0.5, 0.5, 1.0, 0.5])
    return {
        "rain_max_alpha": 1.0,
        "snow_max_alpha": 0.8,
        "edge_alpha": values[3],
        "alpha_rule": "lerp(min(distance^2 / radius^2, 1), max alpha, 0.5) * rain level, per column",
    }


def sky_flash(jar):
    level = javap(jar, "net.minecraft.client.multiplayer.ClientLevel")
    flash = next(b for b in level.split("\n\n") if "float 0.22f" in b)
    values = literals(flash)
    expect("sky flash", values[:1], [0.2199999988079071])
    bolt = literals(method(javap(jar, "net.minecraft.world.entity.LightningBolt"), "public void tick("))
    expect("bolt flash ticks", 2 in bolt, True)
    return {
        "ticks": 2,
        "sky_color_lerp": values[0],
        "flash_color": [0.8, 0.8, 1.0],
        "sky_light_factor": 1.0,
        "rule": "while flashing, sky colour = lerp(0.22, sky, (0.8, 0.8, 1)) and sky light factor = 1; Hide Sky Flashes makes the flash time 0",
    }


def options(jar, archive):
    lang = json.loads(archive.read("assets/minecraft/lang/en_us.json"))
    code = javap(jar, "net.minecraft.client.Options")

    def after(key, n):
        return literals(code.split(f"// String {key}\n", 1)[1].split("// String ", 1)[0])[:n]

    low, high, _, default = expect(
        "Cloud Distance range and default", after("options.renderCloudsDistance", 4), [2, 128, 1, 128]
    )
    flashes = expect("Hide Sky Flashes default", after("options.hideLightningFlashes", 1), [0])[0]
    status = javap(jar, "net.minecraft.client.CloudStatus")
    names = re.findall(r"// String (false|fast|true)\b", status)
    return {
        "clouds": {
            "key": "renderClouds",
            "label": lang["options.renderClouds"],
            "values": [
                {"name": "OFF", "saved": "false", "caption": lang["options.off"]},
                {"name": "FAST", "saved": "fast", "caption": lang["options.clouds.fast"]},
                {"name": "FANCY", "saved": "true", "caption": lang["options.clouds.fancy"]},
            ],
            "saved_names_in_order": names,
            "default": "FANCY",
        },
        "cloud_distance": {
            "key": "cloudRange",
            "label": lang["options.renderCloudsDistance"],
            "value_label": lang["options.chunks"],
            "min": low,
            "max": high,
            "default": default,
        },
        "hide_sky_flashes": {
            "key": "hideLightningFlashes",
            "label": lang["options.hideLightningFlashes"],
            "tooltip": lang["options.hideLightningFlashes.tooltip"],
            "default": bool(flashes),
        },
        "video_quality_order": [
            "biomeBlendRadius",
            "renderDistance",
            "prioritizeChunkUpdates",
            "simulationDistance",
            "ambientOcclusion",
            "cloudStatus",
            "particles",
            "mipmapLevels",
            "entityShadows",
            "entityDistanceScaling",
            "menuBackgroundBlurriness",
            "cloudRange",
            "cutoutLeaves",
            "improvedTransparency",
            "textureFiltering",
            "maxAnisotropyBit",
            "weatherRadius",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("client", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    expect("client jar", hashlib.sha1(args.client.read_bytes()).hexdigest(), CLIENT_SHA1)
    with zipfile.ZipFile(args.client) as archive:
        fixture = {
            "source": {
                "client_sha1": CLIENT_SHA1,
                "tool": "tools/extract_java_atmosphere_fixtures.py",
                "colours": "sRGB values as Java's floats carry them (Java blends them without linearising)",
            },
            "attribute_defaults": attribute_defaults(args.client),
            "dimensions": dimensions(archive),
            "biomes": biomes(archive),
            "day_timeline": day_timeline(archive),
            "weather": weather(args.client),
            "fog": fog(args.client),
            "shaders": shaders(archive),
            "clouds": clouds(args.client),
            "rain": rain(args.client),
            "sky_flash": sky_flash(args.client),
            "options": options(args.client, archive),
        }
    args.output.write_text(json.dumps(fixture, indent=1, sort_keys=False) + "\n")


if __name__ == "__main__":
    main()
