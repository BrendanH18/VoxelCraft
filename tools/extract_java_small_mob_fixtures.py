#!/usr/bin/env python3
"""Extract the endermite's facts from the pinned official Java 26.3 artifact.

Reads `Endermite` (attributes, experience, goals, lifetime, particles,
movement emission, spawn rule), `Mob.createMobAttributes` (the follow range
it inherits), the `EntityTypes.ENDERMITE` builder (category, size, eye
height, peaceful flag), `ThrownEnderpearl.onHit` (the pearl spawn),
`Enderman.registerGoals` (the endermite target goal),
`Enderman.createAttributes`, the `entity.endermite.*` sound events and the
loot table and entity type tags from the data pack.

Requires javap. Unknown bytecode forms fail rather than regenerating a guessed
fixture. No VoxelCraft source or existing fixture is consulted.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1


def method(code, signature):
    """The bytecode of the first method whose declaration contains `signature`."""
    start = code.find(signature)
    if start < 0:
        raise ValueError(f"missing method: {signature}")
    end = code.find("\n\n", start)
    return code[start:end if end >= 0 else len(code)]


def ops(code):
    """Each instruction as `mnemonic operand-comment`, in order."""
    out = []
    for line in code.splitlines():
        match = re.match(r"\s+\d+: (\w+)\s*(.*)", line)
        if match:
            out.append(f"{match[1]} {match[2]}".strip())
    return out


def require_sequence(code, pieces, what):
    """`pieces` appear in this order among the instructions."""
    at = 0
    listing = ops(code)
    for piece in pieces:
        while at < len(listing) and piece not in listing[at]:
            at += 1
        if at == len(listing):
            raise ValueError(f"unrecognized {what}: missing {piece!r}")
        at += 1


def int_constant(code, name):
    match = re.search(r"static final int " + name + r" = (-?\d+);", code)
    if not match:
        raise ValueError(f"missing constant {name}")
    return int(match[1])


def literal(op):
    """The value an instruction pushes, or None for anything else."""
    match = re.match(r"iconst_(m1|\d)$", op)
    if match:
        return -1 if match[1] == "m1" else int(match[1])
    match = re.match(r"(?:bipush|sipush)\s+(-?\d+)$", op)
    if match:
        return int(match[1])
    match = re.match(r"[df]const_(\d)$", op)
    if match:
        return float(match[1])
    match = re.search(r"// (?:float|double) (-?[\d.E-]+)[fd]$", op)
    if match:
        return float(match[1])
    match = re.search(r"// class (\S+)$", op)
    if match and op.startswith("ldc"):
        return "class:" + match[1].rsplit("/", 1)[-1]
    return None


def attributes(code, signature):
    """`Attributes.X` with the double each `add` gives it, in order."""
    found = {}
    listing = ops(method(code, signature))
    for i, op in enumerate(listing):
        match = re.search(r"Field net/minecraft/world/entity/ai/attributes/Attributes\.(\w+):", op)
        if not match:
            continue
        value = literal(listing[i + 1])
        if isinstance(value, float) and "AttributeSupplier$Builder.add:(Lnet/minecraft/core/Holder;D)" in listing[i + 2]:
            found[match[1].lower()] = value
        elif "AttributeSupplier$Builder.add:(Lnet/minecraft/core/Holder;)" in listing[i + 1]:
            found[match[1].lower()] = None
        else:
            raise ValueError(f"unrecognized attribute form in {signature}")
    return found


def goals(code):
    """Each `addGoal(priority, new Goal(...))` of `registerGoals`: selector,
    priority, goal class, literal constructor arguments and whether
    `setAlertOthers` follows."""
    listing = ops(method(code, "protected void registerGoals()"))
    out = []
    starts = [i for i, op in enumerate(listing) if re.search(r"Field (goalSelector|targetSelector):", op)]
    for n, start in enumerate(starts):
        end = starts[n + 1] if n + 1 < len(starts) else len(listing)
        chunk = listing[start:end]
        added = [i for i, op in enumerate(chunk) if "GoalSelector.addGoal" in op]
        if len(added) != 1:
            raise ValueError("unrecognized addGoal form")
        chunk = chunk[:added[0] + 1]
        selector = re.search(r"Field (goalSelector|targetSelector):", chunk[0])[1]
        priority = literal(chunk[1])
        created = re.search(r"// class (\S+)$", chunk[2]) if chunk[2].startswith("new") else None
        if not isinstance(priority, int) or not created or "GoalSelector.addGoal" not in chunk[-1]:
            raise ValueError("unrecognized addGoal form")
        args = []
        for op in chunk[3:]:
            if "invokespecial" in op and "<init>" in op:
                break
            value = literal(op)
            if value is not None:
                args.append(value)
        out.append({
            "selector": selector,
            "priority": priority,
            "goal": created[1].rsplit("/", 1)[-1],
            "args": args,
            "alert_others": any("setAlertOthers" in op for op in chunk),
        })
    return out


def entity_type(code, field):
    """The `EntityType.Builder` calls registering `EntityTypes.<field>`."""
    block = code.split("static {};")[1]
    start = block.find(f"EntityTypeIds.{field}:")
    end = block.find(f"putstatic", start)
    if start < 0 or end < 0 or f"Field {field}:" not in block[end:block.find(chr(10), end)]:
        raise ValueError(f"missing EntityTypes.{field}")
    listing = ops(block[block.rfind(chr(10), 0, start) + 1:end])
    category = None
    sized = eye = None
    peaceful = True
    for i, op in enumerate(listing):
        match = re.search(r"MobCategory\.(\w+):", op)
        if match:
            category = match[1].lower()
        if "Builder.sized:(FF)" in op:
            sized = [literal(listing[i - 2]), literal(listing[i - 1])]
        if "Builder.eyeHeight:(F)" in op:
            eye = literal(listing[i - 1])
        if "Builder.notInPeaceful:" in op:
            peaceful = False
    if category is None or sized is None or eye is None:
        raise ValueError(f"unrecognized EntityTypes.{field} builder")
    return {"category": category, "width": sized[0], "height": sized[1], "eye_height": eye, "allowed_in_peaceful": peaceful}


def pearl_spawn(code):
    body = method(code, "protected void onHit(net.minecraft.world.phys.HitResult)")
    require_sequence(body, [
        "instanceof #",
        "ServerPlayer",
        "RandomSource.nextFloat:",
        "ldc",
        "fcmpg",
        "ServerLevel.isSpawningMonsters:",
        "Difficulty.PEACEFUL:",
        "EntityTypes.ENDERMITE:",
        "EntitySpawnReason.TRIGGERED:",
        "Entity.getX:",
        "Entity.getY:",
        "Entity.getZ:",
        "Entity.getYRot:",
        "Entity.getXRot:",
        "Endermite.snapTo:",
        "ServerPlayer.teleport:",
    ], "ThrownEnderpearl.onHit endermite spawn")
    listing = ops(body)
    roll = next(i for i, op in enumerate(listing) if "RandomSource.nextFloat:" in op)
    chance = literal(listing[roll + 1])
    if not isinstance(chance, float) or not listing[roll + 2].startswith("fcmpg"):
        raise ValueError("unrecognized pearl endermite chance")
    return {
        "chance": chance,
        "owner": "server_player",
        "requires_spawning_monsters": True,
        "excluded_difficulty": "peaceful",
        "spawn_reason": "triggered",
        "at": "owner_position_before_teleport",
        "rotation": "owner",
        "before_teleport": True,
    }


def endermite_class(code):
    init = method(code, "public net.minecraft.world.entity.monster.Endermite(")
    xp = re.search(r"(iconst_\d|bipush\s+\d+)\n\s+\d+: putfield\s+#\d+\s+// Field xpReward:I", init)
    if not xp:
        raise ValueError("unrecognized endermite experience")
    life = int_constant(code, "MAX_LIFE")
    ai = method(code, "public void aiStep()")
    require_sequence(ai, ["Monster.aiStep:", "Level.isClientSide:", "ParticleTypes.PORTAL:", "getRandomX:", "getRandomY:",
                          "getRandomZ:", "isPersistenceRequired:", f"sipush {life}", "discard:"],
                     "Endermite.aiStep")
    particles = re.search(r"iload_1\n\s+\d+: (iconst_\d|bipush\s+\d+)\n\s+\d+: if_icmpge", ai)
    if not particles:
        raise ValueError("unrecognized particle loop")
    emission = re.search(r"getMovementEmission\(\);\n\s+Code:\n\s+\d+: getstatic\s+#\d+\s+// Field net/minecraft/world/entity/Entity\$MovementEmission\.(\w+):", code)
    if not emission:
        raise ValueError("unrecognized movement emission")
    rule = method(code, "public static boolean checkEndermiteSpawnRules(")
    require_sequence(rule, ["checkAnyLightMonsterSpawnRules:", "EntitySpawnReason.isSpawner:", "LevelAccessor.getNearestPlayer:"],
                     "checkEndermiteSpawnRules")
    listing = ops(rule)
    near = next(i for i, op in enumerate(listing) if "getNearestPlayer:" in op)
    distance = literal(listing[near - 2])
    if not isinstance(distance, float):
        raise ValueError("unrecognized endermite spawn distance")
    sounds = {}
    for kind in ("Ambient", "Hurt", "Death"):
        match = re.search(rf"get{kind}Sound\([^)]*\);\n\s+Code:\n\s+\d+: getstatic\s+#\d+\s+// Field net/minecraft/sounds/SoundEvents\.(\w+):", code)
        if not match:
            raise ValueError(f"unrecognized {kind} sound")
        sounds[kind.lower()] = match[1]
    step = method(code, "protected void playStepSound(")
    match = re.search(r"SoundEvents\.(\w+):[^\n]*\n\s+\d+: ldc\s+#\d+\s+// float ([\d.]+)f", step)
    if not match:
        raise ValueError("unrecognized step sound")
    sounds["step"] = match[1]
    if "readAdditionalSaveData" not in code or "// String Lifetime" not in code:
        raise ValueError("endermite no longer saves Lifetime")
    return {
        "experience": literal(xp[1]),
        "max_life": life,
        "life_counts_unless_persistent": True,
        "save_key": "Lifetime",
        "portal_particles_per_tick": literal(particles[1]),
        "particle_spread_scale": 0.5,
        "movement_emission": emission[1].lower(),
        "step_volume": float(match[2]),
        "spawn_rule": {"base": "check_any_light_monster_spawn_rules", "spawner_skips_player_check": True,
                       "no_player_within": distance},
    }, sounds


def sound_ids(code, fields):
    found = {}
    for key, field in fields.items():
        match = re.search(r"ldc(?:_w)?\s+#\d+\s+// String ([\w.]+)\n(?:[^\n]*\n){1,3}?\s+\d+: putstatic\s+#\d+\s+// Field " + field + ":", code)
        if not match:
            raise ValueError(f"missing sound event {field}")
        found[key] = match[1]
    return found


def tag(jar, name, seen=()):
    data = json.loads(jar.read(f"data/minecraft/tags/entity_type/{name}.json"))
    out = set()
    for value in data["values"]:
        value = value if isinstance(value, str) else value["id"]
        if value.startswith("#"):
            inner = value[1:].removeprefix("minecraft:")
            if inner in seen:
                raise ValueError(f"tag cycle at {inner}")
            out |= tag(jar, inner, seen + (name,))
        else:
            out.add(value.removeprefix("minecraft:"))
    return out


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError("not the pinned official Java 26.3 server artifact")
    with zipfile.ZipFile(io.BytesIO(data)) as bundler:
        data = bundler.read("META-INF/versions/26.3/server-26.3.jar")
    with tempfile.TemporaryDirectory(prefix="voxelcraft-java-small-mobs-") as temporary:
        path = Path(temporary) / "reference.jar"
        path.write_bytes(data)

        def inspect(name):
            return subprocess.check_output(
                ["javap", "-classpath", str(path), "-c", "-p", "-constants", "net.minecraft." + name], text=True)

        endermite = inspect("world.entity.monster.Endermite")
        facts, sound_fields = endermite_class(endermite)
        attrs = attributes(endermite, "createAttributes()")
        monster = attributes(inspect("world.entity.monster.Monster"), "createMonsterAttributes()")
        mob = attributes(inspect("world.entity.Mob"), "createMobAttributes()")
        merged = {**mob, **monster, **attrs}
        if any(v is None for v in merged.values()):
            raise ValueError("an endermite attribute keeps its registry default")
        enderman = inspect("world.entity.monster.Enderman")
        enderman_goal = [g for g in goals(enderman) if "class:Endermite" in g["args"]]
        if len(enderman_goal) != 1 or enderman_goal[0]["goal"] != "NearestAttackableTargetGoal":
            raise ValueError("unrecognized enderman endermite goal")
        g = enderman_goal[0]
        if g["args"] != ["class:Endermite", 1, 0]:
            raise ValueError("unrecognized enderman endermite goal arguments")
        enderman_attrs = attributes(enderman, "createAttributes()")
        sounds = sound_ids(inspect("sounds.SoundEvents"), sound_fields)
        registration = entity_type(inspect("world.entity.EntityTypes"), "ENDERMITE")
        pearl = pearl_spawn(inspect("world.entity.projectile.throwableitemprojectile.ThrownEnderpearl"))
        with zipfile.ZipFile(path) as jar:
            loot = json.loads(jar.read("data/minecraft/loot_table/entities/endermite.json"))
            tags = {name: sorted(tag(jar, name)) for name in ("arthropod", "sensitive_to_bane_of_arthropods")}
            bane = json.loads(jar.read("data/minecraft/enchantment/bane_of_arthropods.json"))
        damage = bane["effects"]["minecraft:damage"][0]
        slowness = bane["effects"]["minecraft:post_attack"][0]
        if (damage["effect"]["type"] != "minecraft:add" or damage["effect"]["value"]["type"] != "minecraft:linear"
                or slowness["effect"]["type"] != "minecraft:apply_mob_effect"):
            raise ValueError("unrecognized bane of arthropods effects")
        terms = slowness["requirements"]["terms"]
        bane_facts = {
            "requires_tag": damage["requirements"]["predicate"]["minecraft:entity_type"].removeprefix("#minecraft:"),
            "damage_base": damage["effect"]["value"]["base"],
            "damage_per_level_above_first": damage["effect"]["value"]["per_level_above_first"],
            "slowness": {
                "effect": slowness["effect"]["to_apply"].removeprefix("minecraft:"),
                "min_amplifier": slowness["effect"]["min_amplifier"],
                "max_amplifier": slowness["effect"]["max_amplifier"],
                "min_duration_seconds": slowness["effect"]["min_duration"],
                "max_duration_base_seconds": slowness["effect"]["max_duration"]["base"],
                "max_duration_per_level_above_first": slowness["effect"]["max_duration"]["per_level_above_first"],
                "direct_only": any(t.get("predicate", {}).get("is_direct") is True for t in terms),
            },
            "max_level": bane["max_level"],
        }
    goal_list = goals(endermite)
    return {
        "reference": "Minecraft Java Edition 26.3",
        "server_sha1": SERVER_SHA1,
        "sources": [
            "net.minecraft.world.entity.monster.Endermite",
            "net.minecraft.world.entity.Mob.createMobAttributes",
            "net.minecraft.world.entity.monster.Monster.createMonsterAttributes",
            "net.minecraft.world.entity.EntityTypes (ENDERMITE)",
            "net.minecraft.world.entity.projectile.throwableitemprojectile.ThrownEnderpearl.onHit",
            "net.minecraft.world.entity.monster.Enderman.registerGoals",
            "net.minecraft.sounds.SoundEvents",
            "data/minecraft/loot_table/entities/endermite.json",
            "data/minecraft/tags/entity_type/arthropod.json",
            "data/minecraft/tags/entity_type/sensitive_to_bane_of_arthropods.json",
            "data/minecraft/enchantment/bane_of_arthropods.json",
        ],
        "endermite": {
            "id": "endermite",
            "entity_type": registration,
            "attributes": merged,
            **facts,
            "goals": goal_list,
            "sounds": sounds,
            "loot_table": {"type": loot["type"].removeprefix("minecraft:"), "pools": len(loot.get("pools", []))},
        },
        "pearl_endermite": pearl,
        "enderman_targets_endermites": {
            "priority": g["priority"],
            "must_see": g["args"][1] == 1,
            "must_reach": g["args"][2] == 1,
            "follow_range": enderman_attrs["follow_range"],
            "attack_damage": enderman_attrs["attack_damage"],
        },
        "entity_type_tags": tags,
        "bane_of_arthropods": bane_facts,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("server", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
