#!/usr/bin/env python3
"""Extract the `NeutralMob` game-rule facts from the pinned official Java 26.3 artifact.

Covers which concrete entity classes implement `NeutralMob` (read from the
class files' super classes and interfaces, not from a list), whether each one's
`ResetUniversalAngerTargetGoal` alerts others of its type, each one's
`PERSISTENT_ANGER_TIME` (`TimeUtil.rangeOfSeconds`) and follow range (the
`createAttributes` chain), the box `ServerPlayer.tellNeutralMobsThatIDied`
inflates around the dead player's block and the `forgive_dead_players` gates
around it, and the piglin brain's `UNIVERSAL_ANGER` memory expiry
(`PiglinAi.setAngerTarget`).

Requires javap. Unknown bytecode forms fail rather than regenerating a guessed
fixture. No VoxelCraft source or existing fixture is consulted.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import struct
import subprocess
import tempfile
import zipfile
from extract_java_item_fixtures import SERVER_SHA1

NEUTRAL = 'net/minecraft/world/entity/NeutralMob'
ACC_INTERFACE, ACC_ABSTRACT = 0x0200, 0x0400


def header(data):
    """(access, this, super, interfaces) of a class file, from its constant pool."""
    if data[:4] != b'\xca\xfe\xba\xbe':
        raise ValueError('not a class file')
    count = struct.unpack('>H', data[8:10])[0]
    pool, at, i = {}, 10, 1
    while i < count:
        tag = data[at]
        if tag == 1:
            n = struct.unpack('>H', data[at + 1:at + 3])[0]
            pool[i] = ('utf8', data[at + 3:at + 3 + n].decode('utf-8', 'replace'))
            at += 3 + n
        elif tag == 7:
            pool[i] = ('class', struct.unpack('>H', data[at + 1:at + 3])[0])
            at += 3
        elif tag in (8, 16, 19, 20):
            at += 3
        elif tag == 15:
            at += 4
        elif tag in (3, 4, 9, 10, 11, 12, 17, 18):
            at += 5
        elif tag in (5, 6):
            at += 9
            i += 1
        else:
            raise ValueError(f'unknown constant tag {tag}')
        i += 1
    name = lambda idx: pool[pool[idx][1]][1] if idx else None
    access, this, sup, n = struct.unpack('>HHHH', data[at:at + 8])
    interfaces = [name(struct.unpack('>H', data[at + 8 + 2 * k:at + 10 + 2 * k])[0]) for k in range(n)]
    return access, name(this), name(sup), interfaces


def implementers(jar):
    classes = {}
    for entry in jar.namelist():
        if entry.startswith('net/minecraft/') and entry.endswith('.class'):
            access, this, sup, interfaces = header(jar.read(entry))
            classes[this] = (access, sup, interfaces)

    def neutral(name, seen=()):
        if name == NEUTRAL:
            return True
        if name not in classes or name in seen:
            return False
        _, sup, interfaces = classes[name]
        return any(neutral(p, seen + (name,)) for p in [sup, *interfaces] if p)

    return sorted(name for name, (access, _, _) in classes.items()
                  if not access & (ACC_INTERFACE | ACC_ABSTRACT) and name != NEUTRAL and neutral(name))


def method(code, signature):
    start = code.find(signature)
    if start < 0:
        raise ValueError(f'missing method: {signature}')
    end = code.find('\n\n', start)
    return code[start:end if end >= 0 else len(code)]


def entity_id(cls):
    simple = cls.rsplit('/', 1)[1]
    return re.sub(r'(?<!^)(?=[A-Z])', '_', simple).lower()


def extract(server):
    data = server.read_bytes()
    if hashlib.sha1(data).hexdigest() != SERVER_SHA1:
        raise ValueError('not the pinned Java 26.3 server')
    with zipfile.ZipFile(io.BytesIO(data)) as bundle:
        inner = bundle.read('META-INF/versions/26.3/server-26.3.jar')
    with tempfile.TemporaryDirectory() as scratch:
        jar_path = Path(scratch) / 'inner.jar'
        jar_path.write_bytes(inner)
        cache = {}

        def javap(name):
            if name not in cache:
                cache[name] = subprocess.check_output(
                    ['javap', '-classpath', str(jar_path), '-c', '-p', name.replace('/', '.')], text=True)
            return cache[name]

        def follow_range(name):
            """`FOLLOW_RANGE` from `name.createAttributes`, through the builders it starts from."""
            code = javap(name)
            sig = re.search(r'static net\.minecraft\.world\.entity\.ai\.attributes\.AttributeSupplier\$Builder '
                            r'(create\w*Attributes)\(\);', code)
            if not sig:
                raise ValueError(f'{name}: no attribute builder')
            body = method(code, sig.group(0))
            own = re.search(r'Attributes\.FOLLOW_RANGE:.*\n\s*\d+: ldc2_w\s+#\d+\s+// double ([\d.]+)d', body)
            if own:
                return float(own.group(1))
            base = re.search(r'^\s*0: invokestatic\s+#\d+\s+// Method ([\w/$]+)\.(create\w*Attributes):', body, re.M)
            if not base:
                raise ValueError(f'{name}: attribute chain changed')
            return follow_range_of(base.group(1), base.group(2))

        def follow_range_of(name, builder):
            if builder == 'createLivingAttributes':
                raise ValueError(f'{name}: no follow range before LivingEntity')
            code = javap(name)
            body = method(code, f' {builder}();')
            own = re.search(r'Attributes\.FOLLOW_RANGE:.*\n\s*\d+: ldc2_w\s+#\d+\s+// double ([\d.]+)d', body)
            if own:
                return float(own.group(1))
            base = re.search(r'^\s*0: invokestatic\s+#\d+\s+// Method ([\w/$]+)\.(create\w*Attributes):', body, re.M)
            if not base:
                raise ValueError(f'{name}.{builder}: attribute chain changed')
            return follow_range_of(base.group(1), base.group(2))

        with zipfile.ZipFile(io.BytesIO(inner)) as jar:
            names = implementers(jar)
            loot = set(jar.namelist())
        mobs = {}
        for cls in names:
            code = javap(cls)
            reset = re.findall(r'class net/minecraft/world/entity/ai/goal/target/ResetUniversalAngerTargetGoal\n'
                               r'\s*\d+: dup\n\s*\d+: aload_0\n\s*\d+: iconst_([01])\n', code)
            if len(reset) != 1:
                raise ValueError(f'{cls}: ResetUniversalAngerTargetGoal form changed')
            anger = re.search(r'bipush\s+(\d+)\n\s*\d+: bipush\s+(\d+)\n\s*\d+: invokestatic .*TimeUtil\.rangeOfSeconds'
                              r'.*\n\s*\d+: putstatic .*PERSISTENT_ANGER_TIME', code)
            if not anger:
                raise ValueError(f'{cls}: PERSISTENT_ANGER_TIME form changed')
            mob = entity_id(cls)
            if f'data/minecraft/loot_table/entities/{mob}.json' not in loot:
                raise ValueError(f'{cls}: no entity loot table for {mob}')
            mobs[mob] = {
                'class': cls.replace('/', '.'),
                'alert_others_of_same_type': reset[0] == '1',
                'persistent_anger_seconds': [int(anger.group(1)), int(anger.group(2))],
                'follow_range': follow_range(cls),
            }
        player = javap('net/minecraft/server/level/ServerPlayer')
        tell = method(player, 'private void tellNeutralMobsThatIDied();')
        inflate = [float(v) for v in re.findall(r'ldc2_w\s+#\d+\s+// double ([\d.]+)d', tell)]
        if len(inflate) != 3 or 'AABB."<init>":(Lnet/minecraft/core/BlockPos;)V' not in tell \
                or 'AABB.inflate:(DDD)' not in tell or 'class net/minecraft/world/entity/Mob' not in tell \
                or 'NeutralMob.playerDied' not in method(player, 'lambda$tellNeutralMobsThatIDied$'):
            raise ValueError('tellNeutralMobsThatIDied changed')
        die = method(player, 'public void die(net.minecraft.world.damagesource.DamageSource);')
        gate = re.search(r'GameRules\.FORGIVE_DEAD_PLAYERS:.*\n(?:.*\n){1,4}.*ifeq.*\n(?:.*\n){0,2}'
                         r'.*tellNeutralMobsThatIDied', die)
        neutral = javap(NEUTRAL)
        died = method(neutral, 'default void playerDied(')
        all_players = method(neutral, 'default boolean isAngryAtAllPlayers(')
        if not gate or 'FORGIVE_DEAD_PLAYERS' not in died or 'stopBeingAngry' not in died \
                or 'UNIVERSAL_ANGER' not in all_players or 'getPersistentAngerTarget' not in all_players:
            raise ValueError('NeutralMob rule gates changed')
        piglin = javap('net/minecraft/world/entity/monster/piglin/PiglinAi')
        target = method(piglin, 'protected static void setAngerTarget(')
        expiry = re.findall(r'MemoryModuleType\.(ANGRY_AT|UNIVERSAL_ANGER):.*\n(?:.*\n){0,3}?'
                            r'\s*\d+: ldc2_w\s+#\d+\s+// long (\d+)l', target)
        expiry = dict(expiry)
        if set(expiry) != {'ANGRY_AT', 'UNIVERSAL_ANGER'}:
            raise ValueError('PiglinAi.setAngerTarget changed')
        retaliate = method(piglin, 'protected static void maybeRetaliate(')
        notice = method(piglin, 'public static void angerNearbyPiglins(') + method(piglin, 'lambda$angerNearbyPiglins$')
        if 'setAngerTargetToNearestTargetablePlayerIfFound' not in retaliate \
                or 'broadcastUniversalAnger' not in retaliate or 'UNIVERSAL_ANGER' not in notice:
            raise ValueError('piglin universal anger paths changed')
        brute = javap('net/minecraft/world/entity/monster/piglin/PiglinBruteAi')
        if 'PiglinAi.maybeRetaliate' not in brute:
            raise ValueError('brute retaliation changed')
    return {
        'reference': 'Minecraft Java Edition 26.3',
        'server_sha1': SERVER_SHA1,
        'sources': ['class hierarchy of net.minecraft (NeutralMob implementers)',
                    '<implementer>.registerGoals (ResetUniversalAngerTargetGoal)',
                    '<implementer>.<clinit> (PERSISTENT_ANGER_TIME)', '<implementer>.createAttributes chain',
                    'ServerPlayer.die', 'ServerPlayer.tellNeutralMobsThatIDied',
                    'NeutralMob.playerDied', 'NeutralMob.isAngryAtAllPlayers',
                    'PiglinAi.setAngerTarget', 'PiglinAi.maybeRetaliate', 'PiglinAi.angerNearbyPiglins',
                    'PiglinBruteAi.wasHurtBy'],
        'neutral_mobs': mobs,
        'death_notice': {'around': 'dead player block', 'inflate_xyz': inflate,
                         'only_with_forgive_dead_players': True},
        'piglin': {'angry_at_ticks': int(expiry['ANGRY_AT']), 'universal_anger_ticks': int(expiry['UNIVERSAL_ANGER']),
                   'brutes_retaliate_through_piglin_ai': True},
    }


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('server', type=Path)
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    args.output.write_text(json.dumps(extract(args.server), indent=2) + '\n')


if __name__ == '__main__':
    main()
