#!/usr/bin/env python3
"""Extract Java 26.3 lighting and monster-spawn facts, then reproduce scenes.

Evidence levels, so nothing here is claimed beyond what is checked:

* Data JSON facts (biome spawn lists, dimension types, the day timeline, block
  tags) are read from the pinned official server artifact.
* Bytecode-checked facts: for each rule the script runs javap and requires
  specific instruction forms or constants (for example the sky-light
  dampening default, the 24-block spawn-point distance, and the call sites of
  `Properties.isRedstoneConductor`). A form it does not recognize fails
  instead of regenerating a guessed fixture. These checks confirm that the
  listed constants and call shapes exist; they are not a decompilation or an
  execution of the Java method.
* Hand-reproduced facts: the English `rule` strings, the family expansions
  (all leaves, dye colors, copper weathering/waxing variants), the float
  arithmetic in `sky_darkening_cases`/`detector_cases`, and the scene model
  below were written by hand from the bytecode and data. Only the constants
  they depend on are checked.

Scene expectations come from an independent model: a fixed-point relaxation
of sky and block light (not VoxelCraft's incremental queues) and Java's spawn
checks as exact acceptance probabilities out of 256. No VoxelCraft source or
existing fixture is consulted.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import struct
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1


def require(condition, message):
    if not condition:
        raise ValueError(message)


def method(code, signature):
    parts = code.split("\n  " + signature)
    require(len(parts) == 2, f"missing method: {signature}")
    return parts[1].split("\n\n")[0]


def field_initialization(code, field):
    block = code.split("  static {};")[1]
    end = re.search(r"putstatic\s+#[^\n]*// Field " + field + r":", block)
    require(end, f"missing reference initialization: {field}")
    start = [m.end() for m in re.finditer(r"putstatic[^\n]*", block[: end.start()])]
    return block[start[-1] if start else 0 : end.start()]


def bootstrap_targets(verbose):
    targets = {}
    table = verbose.split("BootstrapMethods:")[-1]
    for m in re.finditer(r"\n\s+(\d+): #\d+ REF_invokeStatic java/lang/invoke/LambdaMetafactory[^\n]*\n\s+Method arguments:\n(.*?)(?=\n\s+\d+: #|\Z)", table, re.S):
        t = re.search(r"REF_invoke\w+ ([\w/$]+)\.([\w$]+)", m[2])
        if t:
            targets[m[1]] = (t[1].split("/")[-1], t[2])
    return targets


def redstone_conductors(properties, blocks, targets):
    """Which Blocks fields override the default `isRedstoneConductor`.

    Bytecode-checked: the default predicate, each call site and its
    predicate (`never` or `always`), and the field assigned after it.
    Helper methods and per-color lambdas are matched to the fields whose
    construction calls them; the full per-color family is hand-reproduced.
    """
    ptargets = bootstrap_targets(properties)
    default = re.search(r"InvokeDynamic #(\d+):test:[^\n]*StatePredicate;\n\s+\d+: putfield\s+#\d+\s+// Field isRedstoneConductor:", properties)
    require(default and ptargets[default[1]] == ("BlockBehaviour$BlockStateBase", "isCollisionShapeFullBlock"),
            "default isRedstoneConductor changed")
    for name, value in [("never", "iconst_0"), ("always", "iconst_1")]:
        require(value in method(blocks, f"private static boolean {name}("), f"{name} predicate changed")

    # Split the code into methods; static initializer windows end at each field store.
    methods, current = {}, None
    for line in blocks.splitlines():
        m = re.match(r"  (?:public|private|protected|static)[^(]*?([\w$]+)\(.*\)(?: throws.*)?;$", line)
        if m:
            current = methods.setdefault(m[1], [])
        elif line.startswith("  static {};"):
            current = methods.setdefault("<clinit>", [])
        elif current is not None:
            current.append(line)

    def predicates(lines):
        found, last = [], None
        for line in lines:
            m = re.search(r"InvokeDynamic #(\d+):test:", line)
            if m:
                last = targets[m[1]][1]
            if "Properties.isRedstoneConductor" in line:
                require(last in ("never", "always"), f"unknown isRedstoneConductor predicate {last}")
                found.append(last)
        return found

    helpers = {name: predicates(lines) for name, lines in methods.items() if name != "<clinit>"}
    helpers = {name: found for name, found in helpers.items() if found}
    by_lambda = {k: v[1] for k, v in targets.items() if v[1] in helpers}
    sites = sum(map(len, helpers.values()))
    result = {"default": "BlockStateBase.isCollisionShapeFullBlock", "never": [], "always": [], "families": {}}
    window = []
    for line in methods["<clinit>"]:
        window.append(line)
        field = re.search(r"putstatic .*Field (\w+):", line)
        if not field:
            continue
        direct = predicates(window)
        sites += len(direct)
        for p in direct:
            result[p].append(field[1].lower())
        for name, found in helpers.items():
            if re.search(r"Method " + re.escape(name) + ":", "\n".join(window)):
                require(len(set(found)) == 1, f"mixed predicates in {name}")
                result[found[0]].append(field[1].lower())
        for ident in re.findall(r"InvokeDynamic #(\d+):apply:", "\n".join(window)):
            if ident in by_lambda:
                name = by_lambda[ident]
                result["families"][field[1].lower()] = {"predicate": helpers[name][0], "factory": name}
        window = []
    result["call_sites_found"] = sites
    for key in ("never", "always"):
        result[key] = sorted(set(result[key]))
    require(result["call_sites_found"] == 26 and result["never"] and result["always"] and result["families"],
            "isRedstoneConductor registrations changed; update the fixture model")
    return result


def count(value):
    if isinstance(value, int):
        return value, value
    require(value.get("type") == "minecraft:uniform", f"unknown count provider {value}")
    return value["min_inclusive"], value["max_inclusive"]


def extract_facts(jar, data):
    def inspect(name, verbose=False):
        flags = ["-c", "-p"] + (["-v"] if verbose else [])
        return subprocess.check_output(["javap", "-classpath", str(jar), *flags, name], text=True)

    def read(name):
        return json.loads(data.read(name))

    facts = {"reference": "Minecraft Java Edition 26.3", "server_sha1": SERVER_SHA1}
    facts["sources"] = [
        "data/minecraft/worldgen/biome/*.json gameplay/natural_mob_spawns",
        "data/minecraft/dimension_type/{overworld,the_nether,the_end}.json",
        "data/minecraft/timeline/day.json gameplay/sky_light_level",
        "data/minecraft/tags/block/prevent_mob_spawning_inside.json",
        "net.minecraft.world.level.NaturalSpawner",
        "net.minecraft.world.entity.monster.Monster.isDarkEnoughToSpawn",
        "net.minecraft.world.entity.SpawnPlacements and SpawnPlacementTypes",
        "net.minecraft.world.level.block.Blocks isValidSpawn and lightLevel registrations",
        "net.minecraft.world.level.block.state.BlockBehaviour light dampening and default isValidSpawn",
        "net.minecraft.world.level.lighting.LightEngine.getOpacity and ChunkSkyLightSources.isEdgeOccluded",
        "net.minecraft.world.level.Level.updateSkyBrightness, EnvironmentAttributes and WeatherAttributes",
        "net.minecraft.world.entity.EntityType.isBlockDangerous, Mob.getMaxSpawnClusterSize",
    ]

    # Light propagation.
    opacity = method(inspect("net.minecraft.world.level.lighting.LightEngine"), "protected int getOpacity(")
    require("getLightDampening" in opacity and "iconst_1" in opacity and "Math.max" in opacity, "opacity rule changed")
    occluded = method(inspect("net.minecraft.world.level.lighting.ChunkSkyLightSources"), "private static boolean isEdgeOccluded(")
    require("getLightDampening" in occluded and "ifeq" in occluded, "sky source rule changed")
    dampening = method(inspect("net.minecraft.world.level.block.state.BlockBehaviour"), "protected int getLightDampening(")
    require(re.search(r"isSolidRender[\s\S]*bipush\s+15[\s\S]*propagatesSkylightDown[\s\S]*iconst_0[\s\S]*iconst_1", dampening), "dampening default changed")
    facts["light"] = {
        "step_cost": "max(1, light dampening of the cell entered)",
        "sky_sources": "every cell above the highest block with nonzero light dampening is level 15",
        "dampening": "15 for solid-render blocks, 0 where skylight propagates down (non-full shape, no fluid, glass), otherwise 1",
        "max_level": 15,
    }

    # Emission constants used by the scenes.
    blocks = inspect("net.minecraft.world.level.block.Blocks")
    blocks_verbose = inspect("net.minecraft.world.level.block.Blocks", True)
    targets = bootstrap_targets(blocks_verbose)
    emission = {}
    for field, name in [("TORCH", "torch"), ("GLOWSTONE", "glowstone"), ("JACK_O_LANTERN", "jack_o_lantern"), ("MAGMA_BLOCK", "magma_block")]:
        init = field_initialization(blocks, field)
        ids = re.findall(r"InvokeDynamic #(\d+):applyAsInt", init)
        require(len(ids) == 1, f"unrecognized light level for {field}")
        lam = method(blocks, f"private static int {targets[ids[0]][1]}(")
        value = re.findall(r"(?:bipush\s+(\d+)|iconst_(\d))", lam)
        require(len(value) == 1, f"unrecognized light level lambda for {field}")
        emission[name] = int(value[0][0] or value[0][1])
    lamp = field_initialization(blocks, "REDSTONE_LAMP")
    lit = re.search(r"bipush\s+(\d+)\s*\n[^\n]*litBlockEmission", lamp)
    require(lit, "unrecognized redstone lamp emission")
    emission["lit_redstone_lamp"] = int(lit[1])
    facts["light"]["emission"] = emission

    # Isvalidspawn overrides on Blocks.
    names = {"ocelotOrParrot": "ocelot_or_parrot", "never": "never", "always": "always"}

    def predicate_name(meth):
        if meth in names:
            return names[meth]
        body = method(blocks, f"private static boolean {meth}(")
        if "EntityTypes.POLAR_BEAR" in body:
            return "polar_bear"
        if "fireImmune" in body:
            return "fire_immune"
        return None
    overrides = {}
    pending = []
    current = None
    for line in blocks.splitlines():
        m = re.match(r"\s+(?:public|private|protected).*? (\w+)\(.*\);$", line)
        if m:
            for p in pending:
                overrides.setdefault(p, []).append(f"{current}()")
            pending, current = [], m[1]
        m = re.search(r"InvokeDynamic #(\d+):test:\(\)Lnet/minecraft/world/level/block/state/BlockBehaviour\$StateArgumentPredicate", line)
        if m:
            predicate = targets[m[1]][1]
        if "Properties.isValidSpawn" in line:
            name = predicate_name(predicate)
            require(name, f"unknown isValidSpawn predicate {predicate}")
            pending.append(name)
        m = re.search(r"putstatic .*Field (\w+):Lnet/minecraft/world/level/block/Block;", line)
        if m and pending:
            for p in pending:
                overrides.setdefault(p, []).append(m[1].lower())
            pending = []
    default = method(inspect("net.minecraft.world.level.block.state.BlockBehaviour$Properties"), "private static boolean lambda$new$4(")
    require("Direction.UP" in default and "isFaceSturdy" in default and re.search(r"bipush\s+14", default), "default isValidSpawn changed")
    facts["spawn_floor"] = {
        "default": "top face sturdy and light emission below 14",
        **{k: sorted(set(v)) for k, v in sorted(overrides.items())},
    }
    facts["redstone_conductor"] = redstone_conductors(
        inspect("net.minecraft.world.level.block.state.BlockBehaviour$Properties", True), blocks, targets)
    dangerous = method(inspect("net.minecraft.world.entity.EntityType"), "public boolean isBlockDangerous(")
    facts["dangerous_blocks"] = ["burning blocks unless fire immune"] + [
        b.lower() for b in re.findall(r"Field net/minecraft/world/level/block/Blocks\.(\w+):", dangerous)
    ]
    facts["prevent_mob_spawning_inside"] = read("data/minecraft/tags/block/prevent_mob_spawning_inside.json")["values"]

    # Monster light test and sky darkening.
    dark = method(inspect("net.minecraft.world.entity.monster.Monster"), "public static boolean isDarkEnoughToSpawn(")
    for token in ["LightLayer.SKY", "bipush        32", "monsterSpawnBlockLightLimit", "LightLayer.BLOCK", "isThundering", "bipush        10", "monsterSpawnLightTest"]:
        require(token in dark, f"isDarkEnoughToSpawn changed: {token}")
    level_code = inspect("net.minecraft.world.level.Level")
    thundering = method(level_code, "public boolean isThundering(")
    threshold = re.search(r"canHaveWeather[\s\S]*fconst_1[\s\S]*getThunderLevel:\(F\)F\n[^\n]*f2d\n[^\n]*// double ([\d.]+)d\n[^\n]*dcmpl", thundering)
    require(threshold, "isThundering changed")
    thunder_level = method(level_code, "public float getThunderLevel(")
    require(re.search(r"oThunderLevel[\s\S]*thunderLevel[\s\S]*Mth\.lerp[\s\S]*getRainLevel:\(F\)F\n[^\n]*fmul", thunder_level), "thunder level changed")
    facts["monster_light"] = {
        "sky_roll": 32,
        "rule": "fail if sky > nextInt(32); fail if block light > the dimension's limit (when below 15); pass if max(sky - darken, block) <= the dimension's light test sample",
        "thunder_darken": 10,
        "thundering": "Level.isThundering: canHaveWeather and (double) getThunderLevel(1) > thundering_above; getThunderLevel = lerp(oThunderLevel, thunderLevel) * getRainLevel",
        "thundering_above": float(threshold[1]),
    }
    sky = method(level_code, "public void updateSkyBrightness(")
    require("float 15.0f" in sky and "SKY_LIGHT_LEVEL" in sky and "f2i" in sky, "sky darkening changed")
    attrs = inspect("net.minecraft.world.attribute.EnvironmentAttributes")
    default_level = re.search(r"String gameplay/sky_light_level\n[^\n]*\n[^\n]*\n\s+\d+: ldc_w?\s+#\d+\s+// float ([\d.]+)f", attrs)
    require(default_level, "unrecognized sky light level default")
    weather = inspect("net.minecraft.world.attribute.WeatherAttributes")
    blends = re.findall(r"SKY_LIGHT_LEVEL:[^\n]*\n[^\n]*ALPHA_BLEND[^\n]*\n[^\n]*new[^\n]*\n[^\n]*dup\n[^\n]*// float ([\d.]+)f\n[^\n]*// float ([\d.]+)f", weather)
    require(len(blends) == 2, "unrecognized weather sky light blends")
    day = read("data/minecraft/timeline/day.json")["tracks"]["minecraft:gameplay/sky_light_level"]
    require(day["modifier"] == "multiply", "day sky light is no longer a multiplier")
    require(day.get("ease", "linear") == "linear", "day sky light easing changed")
    keyframes = inspect("net.minecraft.util.KeyframeTrack")
    require("EasingType.LINEAR" in keyframes and "String ease" in keyframes, "default keyframe easing changed")
    require("WeatherAccess.rainLevel:()F" in weather and "WeatherAccess.thunderLevel:()F" in weather
            and "fsub" in method(weather, "private static java.lang.Object lambda$addLayer$0("),
            "weather layering changed")
    alpha_blend = inspect("net.minecraft.world.attribute.modifier.FloatModifier$1")
    require("Mth.lerp:(FFF)F" in method(alpha_blend, "public java.lang.Float apply("), "weather alpha blend changed")
    facts["sky_darken"] = {
        "rule": "int(15 - gameplay/sky_light_level)",
        "default_level": float(default_level[1]),
        "period_ticks": read("data/minecraft/timeline/day.json")["period_ticks"],
        "interpolation": "linear float",
        "day_multiplier_keyframes": [[k["ticks"], k["value"]] for k in day["keyframes"]],
        "rain_blend": {"toward": float(blends[0][0]), "alpha": float(blends[0][1])},
        "thunder_blend": {"toward": float(blends[1][0]), "alpha": float(blends[1][1])},
        "weather_rule": "lerp(rainLevel - thunderLevel, base, rainModifier(base)); then lerp(thunderLevel, base, thunderModifier(base))",
    }
    facts["sky_darken"]["cases"] = sky_darkening_cases(facts["sky_darken"], facts["monster_light"])

    # Daylight detectors share skyDarken but add the sun-angle cosine.
    detector = method(inspect("net.minecraft.world.level.block.DaylightDetectorBlock"), "private static void updateSignalStrength(")
    for token in ["Level.getEffectiveSkyBrightness", "EnvironmentAttributes.SUN_ANGLE", "float 0.017453292f", "INVERTED",
                  "bipush        15", "float 3.1415927f", "float 6.2831855f", "float 0.2f", "Mth.cos:(D)F", "Math.round:(F)I", "Mth.clamp:(III)I"]:
        require(token in detector, f"daylight detector changed: {token}")
    effective = method(inspect("net.minecraft.world.level.LevelReader"), "public default int getEffectiveSkyBrightness(")
    require("LightLayer.SKY" in effective and "getSkyDarken" in effective and "Math.max" in effective, "effective sky brightness changed")
    mth = inspect("net.minecraft.util.Mth")
    cos = method(mth, "public static float cos(double)")
    require("double 10430.378350470453d" in cos and "double 16384.0d" in cos and "long 65535l" in cos, "Mth.cos changed")
    require("Math.sin:(D)D" in method(mth, "private static void lambda$static$0(float[])"), "Mth sine table changed")
    bezier = inspect("net.minecraft.util.EasingType$CubicBezier")
    require("iconst_4" in method(bezier, "private float solveT(") and "float -0.25f" in bezier and "float 1.0E-5f" in bezier,
            "cubic bezier solver changed")
    sun = read("data/minecraft/timeline/day.json")["tracks"]["minecraft:visual/sun_angle"]
    require(list(sun["ease"]) == ["cubic_bezier"] and sun.get("modifier") is None, "sun angle track changed")
    attrs_default = re.search(r"String visual/sun_angle\n[^\n]*\n[^\n]*\n\s+\d+: fconst_0", attrs)
    require(attrs_default, "sun angle default changed")
    facts["daylight_detector"] = {
        "rule": "effective = max(0, sky light - skyDarken); inverted: 15 - effective; otherwise if effective > 0: "
                "a = sun_angle * 0.017453292f, a += ((a < 3.1415927f ? 0 : 6.2831855f) - a) * 0.2f, "
                "round(effective * Mth.cos(a)); clamp 0..15",
        "mth_cos": "SIN[(long)(x * 10430.378350470453 + 16384.0) & 65535], SIN[i] = (float) sin(i / 10430.378350470453)",
        "sun_angle_default": 0.0,
        "sun_angle_keyframes": [[k["ticks"], k["value"]] for k in sun["keyframes"]],
        "sun_angle_ease": sun["ease"]["cubic_bezier"],
    }
    facts["daylight_detector"]["cases"] = detector_cases(facts["daylight_detector"], facts["sky_darken"]["period_ticks"])
    attrs_text = inspect("net.minecraft.world.attribute.EnvironmentAttributes")
    factor = re.search(r"String visual/sky_light_factor\n[^\n]*AttributeTypes\.FLOAT[^\n]*\n[^\n]*\n[^\n]*fconst_1", attrs_text)
    require(factor, "visual sky light factor default changed")
    sky_factor_default = 1.0
    dims = {}
    for dim in ["overworld", "the_nether", "the_end"]:
        d = read(f"data/minecraft/dimension_type/{dim}.json")
        level = d["monster_spawn_light_level"]
        dims[dim] = {
            "has_skylight": d["has_skylight"],
            "visual_sky_light_factor": d.get("attributes", {}).get("minecraft:visual/sky_light_factor", sky_factor_default),
            "monster_spawn_block_light_limit": d["monster_spawn_block_light_limit"],
            "monster_spawn_light_level": list(count(level)),
            "min_y": d["min_y"],
            "height": d["height"],
        }
    facts["dimensions"] = dims

    # The natural spawn cycle.
    spawner = inspect("net.minecraft.world.level.NaturalSpawner")
    random_pos = method(spawner, "private static net.minecraft.core.BlockPos getRandomPosWithin(")
    require(random_pos.count("bipush        16") == 2 and "WORLD_SURFACE" in random_pos and "randomBetweenInclusive" in random_pos, "start sampling changed")
    position = method(spawner, "public static void spawnCategoryForPosition(net.minecraft.world.entity.MobCategory, net.minecraft.server.level.ServerLevel, net.minecraft.world.level.chunk.ChunkAccess")
    require("isRedstoneConductor" in position and "iconst_3" in position and "float 4.0f" in position and position.count("bipush        6") == 5, "pack walk changed")
    require("getMaxSpawnClusterSize" in position and "isMaxGroupSizeReached" in position, "pack limits changed")
    distance = method(spawner, "private static boolean isRightDistanceToPlayerAndSpawnPoint(")
    require("double 576.0d" in distance and "double 24.0d" in distance, "player distance changed")
    spawn_point = re.search(r"ServerLevel\.getRespawnData[\s\S]*RespawnData\.dimension[\s\S]*ServerLevel\.dimension"
                            r"[\s\S]*RespawnData\.pos[\s\S]*// double ([\d.]+)d\n[^\n]*BlockPos\.closerToCenterThan", distance)
    require(spawn_point, "spawn point distance changed")
    chunk = method(spawner, "public static void spawnCategoryForChunk(")
    require("getMinY" in chunk and "iconst_1" in chunk, "minimum start height changed")
    cluster = method(inspect("net.minecraft.world.entity.Mob"), "public int getMaxSpawnClusterSize(")
    require("iconst_4" in cluster, "default cluster size changed")
    facts["natural_spawner"] = {
        "start": "x, z uniform in a 16x16 chunk; y uniform from min_y to WORLD_SURFACE + 1; nothing below min_y + 1",
        "start_conductor_stops": True,
        "packs": 3,
        "first_pack_steps": "ceil(nextFloat() * 4)",
        "step": "x and z += nextInt(6) - nextInt(6)",
        "min_player_distance_squared": 576.0,
        "spawn_point_min_distance": float(spawn_point[1]),
        "pack_steps_after_choice": "the chosen entry's count",
        "max_cluster": 4,
    }

    # Spawn placement registrations of the monsters that use these rules.
    placements = inspect("net.minecraft.world.entity.SpawnPlacements")
    ptargets = bootstrap_targets(inspect("net.minecraft.world.entity.SpawnPlacements", True))
    rules = {}
    etype = place = pred = None
    for line in placements.splitlines():
        m = re.search(r"Field net/minecraft/world/entity/EntityTypes\.(\w+):", line)
        if m:
            etype = m[1]
        m = re.search(r"Field net/minecraft/world/entity/SpawnPlacementTypes\.(\w+):", line)
        if m:
            place = m[1]
        m = re.search(r"InvokeDynamic #(\d+):test", line)
        if m:
            pred = ".".join(ptargets[m[1]])
        if "Method register:" in line:
            rules[etype.lower()] = {"placement": place, "rule": pred}
    wanted = ["zombie", "zombie_villager", "husk", "skeleton", "creeper", "spider", "enderman", "witch", "slime", "drowned"]
    facts["spawn_rules"] = {k: rules[k] for k in wanted}

    # Biome monster lists.
    lists = {}
    for name in sorted(data.namelist()):
        if not (name.startswith("data/minecraft/worldgen/biome/") and name.endswith(".json")):
            continue
        attribute = read(name).get("attributes", {}).get("minecraft:gameplay/natural_mob_spawns")
        require(attribute and attribute.get("modifier") == "overlay", f"unexpected spawn attribute in {name}")
        monsters = attribute["argument"]["spawns_by_category"].get("monster", [])
        lists[name.split("/")[-1][:-5]] = [
            {"type": e["type"].removeprefix("minecraft:"), "weight": e["weight"], "min": count(e["count"])[0], "max": count(e["count"])[1]}
            for e in monsters
        ]
    facts["monster_spawns"] = lists
    return facts


def sky_darkening_cases(facts, monster_light):
    """Explicit clock/weather scenarios, evaluated in Java float precision.
    `thunder` is getThunderLevel(1); thundering is derived from it."""
    thunder_darken = monster_light["thunder_darken"]
    f32 = lambda n: struct.unpack("f", struct.pack("f", n))[0]
    def lerp(alpha, a, b):
        return f32(a + f32(alpha * f32(b - a)))
    period = facts["period_ticks"]
    keys = [(tick, f32(value)) for tick, value in facts["day_multiplier_keyframes"]]
    out = []
    for tick in [0, 1, 132, 133, 134, 6000, 11866, 11867, 11868, 12768, 13669, 13670, 18000,
                 22329, 22330, 22331, 23231, 23999, 24000]:
        t = tick % period
        if t < keys[0][0]:
            lo, hi, t = keys[-1], (keys[0][0] + period, keys[0][1]), t + period
        else:
            i = max(i for i, (k, _) in enumerate(keys) if k <= t)
            lo = keys[i]
            hi = keys[i + 1] if i + 1 < len(keys) else (keys[0][0] + period, keys[0][1])
        alpha = f32(f32(t - lo[0]) / f32(hi[0] - lo[0]))
        day_level = f32(f32(facts["default_level"]) * lerp(alpha, lo[1], hi[1]))
        for rain, thunder in [(0., 0.), (1., 0.), (1., 1.), (0.5, 0.), (0.7, 0.35), (1., 0.9), (1., 0.95)]:
            thundering = f32(thunder) > monster_light["thundering_above"]
            level = day_level
            rain_only = f32(f32(rain) - f32(thunder))
            if rain_only > 0:
                blend = facts["rain_blend"]
                level = lerp(rain_only, level, lerp(f32(blend["alpha"]), level, f32(blend["toward"])))
            if thunder > 0:
                blend = facts["thunder_blend"]
                level = lerp(f32(thunder), level, lerp(f32(blend["alpha"]), level, f32(blend["toward"])))
            darken = int(f32(15. - level))
            out.append({"tick": tick, "rain": rain, "thunder": thunder, "thundering": thundering,
                        "sky_darken": darken, "monster_darken": thunder_darken if thundering else darken})
    return out


def detector_cases(facts, period):
    """Detector outputs from the timeline, Java float operations and the sine table."""
    import math
    f32 = lambda n: struct.unpack("f", struct.pack("f", n))[0]
    x1, y1, x2, y2 = (f32(v) for v in facts["sun_angle_ease"])

    def curve(c1, c2):
        return (f32(f32(f32(3. * c1) - f32(3. * c2)) + 1.), f32(f32(-6. * c1) + f32(3. * c2)), f32(3. * c1))

    xc, yc = curve(x1, x2), curve(y1, y2)
    sample = lambda c, t: f32(f32(f32(f32(f32(c[0] * t) + c[1]) * t) + c[2]) * t)
    gradient = lambda c, t: f32(f32(f32(f32(f32(3. * c[0]) * t) + f32(2. * c[1])) * t) + c[2])

    def ease(x):
        t = x
        for _ in range(4):
            err = f32(sample(xc, t) - x)
            if abs(err) < f32(1e-5):
                return sample(yc, t)
            g = gradient(xc, t)
            if g < f32(1e-5):
                break
            t = f32(t - min(0.25, max(-0.25, f32(err / g))))
        lo, hi = 0., 1.
        while lo < hi:
            err = f32(sample(xc, t) - x)
            if abs(err) < f32(1e-5):
                break
            if err < 0:
                lo = t
            else:
                hi = t
            t = f32(f32(hi + lo) / 2.)
        return sample(yc, t)

    (start_tick, start), (end_tick, end) = sorted(facts["sun_angle_keyframes"], key=lambda k: k[1])
    require(start_tick == end_tick and start == 0 and end == 360, "unexpected sun angle keyframes")

    def sun_angle(tick):
        since = (tick - start_tick) % period
        if since == 0:
            return f32(start)
        return f32(start + f32(ease(f32(f32(since) / f32(period))) * f32(end - start)))

    def cos(x):
        i = int(x * 10430.378350470453 + 16384.0) & 65535
        return f32(math.sin(i / 10430.378350470453))

    out = []
    for tick in [0, 1000, 3000, 5999, 6000, 6001, 9000, 11000, 12000, 12500, 13000, 13500, 17999, 18000, 18001,
                 22500, 23000, 23500, 23999]:
        angle_deg = sun_angle(tick)
        for effective in [15, 12, 4, 0]:
            for inverted in [False, True]:
                power = effective
                if inverted:
                    power = 15 - power
                elif power > 0:
                    a = f32(angle_deg * f32(0.017453292))
                    target = 0. if a < f32(3.1415927) else f32(6.2831855)
                    a = f32(a + f32(f32(target - a) * f32(0.2)))
                    power = math.floor(f32(f32(power) * cos(a)) + 0.5)
                out.append({"tick": tick, "sun_angle": angle_deg, "effective_sky": effective, "inverted": inverted,
                            "power": max(0, min(15, power))})
    return out


# Independent reference model ---------------------------------------------

GROUND = 63
EMISSION = {}
# Normalized Java block properties used by the scenes (see facts["light"] and
# facts["spawn_floor"]): dampening, collision, top-face sturdiness.
PROPERTIES = {
    "air": dict(damp=0, full=False, sturdy=False, floor=None),
    "stone": dict(damp=15, full=True, sturdy=True, floor=None),
    "glass": dict(damp=0, full=True, sturdy=True, floor="never"),
    "oak_leaves": dict(damp=1, full=True, sturdy=True, floor="ocelot_or_parrot"),
    "bedrock": dict(damp=15, full=True, sturdy=True, floor="never"),
    "ice": dict(damp=1, full=True, sturdy=True, floor="polar_bear"),
    "magma_block": dict(damp=15, full=True, sturdy=True, floor="fire_immune"),
    "soul_sand": dict(damp=15, full=False, sturdy=False, floor="always"),
    "stone_slab": dict(damp=0, full=False, sturdy=False, floor=None),
    "stone_brick_stairs": dict(damp=0, full=False, sturdy=False, floor=None),
    "farmland": dict(damp=0, full=False, sturdy=False, floor=None),
    "glowstone": dict(damp=15, full=True, sturdy=True, floor=None),
    "jack_o_lantern": dict(damp=15, full=True, sturdy=True, floor="always"),
    "redstone_lamp": dict(damp=15, full=True, sturdy=True, floor="always"),
    "lit_redstone_lamp": dict(damp=15, full=True, sturdy=True, floor="always"),
    "torch": dict(damp=0, full=False, sturdy=False, floor=None),
    "rail": dict(damp=0, full=False, sturdy=False, floor=None, rail=True),
    "water": dict(damp=1, full=False, sturdy=False, floor=None, fluid=True),
}


class Scene:
    def __init__(self, edits):
        self.cells = {}
        for name, lo, hi in edits:
            require(name in PROPERTIES, f"unknown scene block {name}")
            for x in range(lo[0], hi[0] + 1):
                for y in range(lo[1], hi[1] + 1):
                    for z in range(lo[2], hi[2] + 1):
                        self.cells[(x, y, z)] = name
        self.height = {}
        for (x, y, z), name in self.cells.items():
            self.height.setdefault((x, z), None)
        for key in list(self.height):
            ys = [y for (x, y, z), n in self.cells.items() if (x, z) == key]
            top = max(max(ys), GROUND)
            h = None
            for y in range(top, -65, -1):
                if PROPERTIES[self.block((key[0], y, key[1]))]["damp"] > 0:
                    h = y
                    break
            self.height[key] = h
        self.sky = self.relax(lambda p: 15 if self.source(p) else 0)
        self.light = self.relax(lambda p: EMISSION.get(self.block(p), 0))

    def block(self, p):
        if p in self.cells:
            return self.cells[p]
        return "stone" if p[1] <= GROUND else "air"

    def source(self, p):
        h = self.height.get((p[0], p[2]), GROUND)
        return h is None or p[1] > h

    def relax(self, own):
        # Every cell that is neither opaque nor fixed may change; outside the
        # edited region (flat terrain) values are exact from `own` alone.
        active = set()
        for (x, y, z) in self.cells:
            for dx in range(-15, 16):
                for dy in range(-15, 16):
                    for dz in range(-15, 16):
                        if abs(dx) + abs(dy) + abs(dz) <= 15:
                            q = (x + dx, y + dy, z + dz)
                            if PROPERTIES[self.block(q)]["damp"] < 15:
                                active.add(q)
        level = {p: own(p) for p in active}
        get = lambda q: level.get(q, own(q))
        changed = True
        while changed:
            changed = False
            for p in active:
                cost = max(1, PROPERTIES[self.block(p)]["damp"])
                best = max([own(p)] + [get((p[0] + d[0], p[1] + d[1], p[2] + d[2])) - cost for d in SIDES])
                if best > level[p]:
                    level[p] = best
                    changed = True
        return get

    def spawn_odds(self, p, darken, limit=0, light_max=7):
        """Zombie acceptance out of 256 at feet cell p (32 sky rolls x 8 light rolls)."""
        below, at, above = self.block((p[0], p[1] - 1, p[2])), self.block(p), self.block((p[0], p[1] + 1, p[2]))
        rule = PROPERTIES[below]["floor"]
        floor = rule == "always" or (rule is None and PROPERTIES[below]["sturdy"] and EMISSION.get(below, 0) < 14)
        empty = all(not PROPERTIES[b]["full"] and not PROPERTIES[b].get("fluid") and not PROPERTIES[b].get("rail") for b in (at, above))
        sky, block = self.sky(p), self.light(p)
        if not floor or not empty or block > limit:
            return 0
        raw = max(sky - darken, block)
        return (32 - sky) * max(0, light_max + 1 - raw)


SIDES = [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)]


def box(name, lo, hi):
    return [name, lo, hi]


def scenes():
    sealed_cave = [box("air", [8, 30, 8], [16, 33, 16])]
    torch_tunnel = sealed_cave + [box("air", [17, 30, 12], [40, 31, 12])]
    shaft = [box("air", [20, 40, 16], [20, 63, 16]), box("air", [21, 40, 16], [36, 41, 16])]
    pockets = []
    floors = ["stone", "glass", "oak_leaves", "bedrock", "ice", "magma_block", "soul_sand", "stone_slab",
              "stone_brick_stairs", "farmland", "glowstone", "jack_o_lantern", "redstone_lamp", "lit_redstone_lamp"]
    for i, floor in enumerate(floors):
        x = 4 + 3 * i
        pockets += [box("air", [x, 30, 8], [x, 31, 8]), box(floor, [x, 29, 8], [x, 29, 8])]
    for i, (feet, head) in enumerate([("rail", "air"), ("water", "air"), ("air", "water")]):
        x = 4 + 3 * i
        pockets += [box("air", [x, 30, 20], [x, 31, 20]), box(feet, [x, 30, 20], [x, 30, 20]), box(head, [x, 31, 20], [x, 31, 20])]
    probe = lambda *cells: [list(c) for c in cells]
    return [
        {
            "name": "sealed_cave",
            "steps": [{"edits": sealed_cave, "probes": probe((9, 30, 9), (12, 30, 12), (16, 30, 16), (12, 33, 12))}],
        },
        {
            "name": "torch_suppression_and_removal",
            "steps": [
                {"edits": torch_tunnel + [box("torch", [12, 30, 12], [12, 30, 12])],
                 "probes": probe((9, 30, 9), (16, 30, 16), (24, 30, 12), (25, 30, 12), (26, 30, 12), (27, 30, 12), (40, 30, 12))},
                {"edits": [box("air", [12, 30, 12], [12, 30, 12])],
                 "probes": probe((9, 30, 9), (16, 30, 16), (25, 30, 12), (40, 30, 12))},
            ],
        },
        {
            "name": "sky_shaft_and_side_tunnel",
            "steps": [
                {"edits": shaft, "probes": probe((20, 40, 16), (20, 55, 16), (21, 40, 16), (24, 40, 16), (28, 40, 16), (31, 40, 16), (34, 40, 16), (36, 40, 16))},
                {"edits": [box("glass", [20, 63, 16], [20, 63, 16])], "probes": probe((20, 40, 16), (24, 40, 16))},
                {"edits": [box("oak_leaves", [20, 63, 16], [20, 63, 16])], "probes": probe((20, 64, 16), (20, 55, 16), (20, 50, 16), (20, 49, 16), (24, 40, 16))},
                {"edits": [box("stone", [20, 63, 16], [20, 63, 16])], "probes": probe((20, 40, 16), (24, 40, 16), (34, 40, 16))},
                {"edits": [box("air", [20, 63, 16], [20, 63, 16])], "probes": probe((20, 40, 16), (28, 40, 16))},
            ],
        },
        {
            "name": "overhang",
            "steps": [{"edits": [box("stone", [14, 70, 14], [18, 70, 18])], "probes": probe((16, 64, 16), (15, 64, 16), (14, 64, 16), (13, 64, 16), (16, 69, 16))}],
        },
        {
            "name": "loaded_border_tunnel",
            "unload_column": [1, 0],
            "steps": [{"edits": [box("air", [36, 50, 16], [36, 63, 16]), box("air", [20, 50, 16], [35, 51, 16])],
                       "probes": probe((36, 50, 16), (31, 50, 16), (28, 50, 16), (24, 50, 16), (21, 50, 16))}],
        },
        {
            "name": "floors_and_occupied_cells",
            "steps": [{"edits": pockets, "probes": probe(*[(4 + 3 * i, 30, 8) for i in range(len(floors))], *[(4 + 3 * i, 30, 20) for i in range(3)])}],
        },
    ]


def reproduce(facts):
    EMISSION.update({"torch": facts["light"]["emission"]["torch"], "glowstone": facts["light"]["emission"]["glowstone"],
                     "jack_o_lantern": facts["light"]["emission"]["jack_o_lantern"],
                     "lit_redstone_lamp": facts["light"]["emission"]["lit_redstone_lamp"],
                     "magma_block": facts["light"]["emission"]["magma_block"]})
    overworld = facts["dimensions"]["overworld"]
    require(overworld["monster_spawn_block_light_limit"] == 0 and overworld["monster_spawn_light_level"] == [0, 7], "overworld spawn light changed")
    out = []
    for scene in scenes():
        edits = []
        steps = []
        for step in scene["steps"]:
            edits += step["edits"]
            model = Scene(edits)
            probes = []
            for p in step["probes"]:
                p = tuple(p)
                probes.append({"cell": list(p), "sky": model.sky(p), "block": model.light(p),
                               "zombie_night_of_256": model.spawn_odds(p, 11), "zombie_noon_of_256": model.spawn_odds(p, 0)})
            steps.append({"edits": step["edits"], "probes": probes})
        out.append({k: v for k, v in scene.items() if k != "steps"} | {"steps": steps})
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path, help="official Java 26.3 server.jar (bundler)")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    data = args.server.read_bytes()
    require(hashlib.sha1(data).hexdigest() == SERVER_SHA1, "not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        inner = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-light-") as temporary:
        jar = Path(temporary) / "reference.jar"
        jar.write_bytes(inner)
        with zipfile.ZipFile(jar) as reference:
            facts = extract_facts(jar, reference)
    facts["model"] = ("Scenes are original. Each step adds its edits to a flat world (stone up to y = 63, air above) and "
                      "is reproduced from scratch: light by fixed-point relaxation, spawn odds out of 256 for a zombie "
                      "with Java's ON_GROUND checks and isDarkEnoughToSpawn at sky darkening 11 (clear night) and 0 (noon).")
    facts["scenes"] = reproduce(facts)
    args.output.write_text(json.dumps(facts, indent=2) + "\n")


if __name__ == "__main__":
    main()
