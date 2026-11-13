#!/usr/bin/env python3
"""Generate Java 26.3 `MobEffectInstance` behaviour cases from the decompiled source.

`MobEffectInstance` is behaviour, not data, and the official 26.3 jar needs a
Java 25 runtime to execute, so this tool carries a line-by-line model of the
methods the game must match, transcribed from the decompiled (Vineflower)
source of the pinned version:

* `MobEffectInstance.update`, `isShorterDurationThan`, `tickServer`,
  `tickDownDuration`, `mapDuration`, `downgradeToHiddenEffect`, `endsWithin`
  and `compareTo` (with Guava's `ComparisonChain`: `compareFalseFirst` and
  `compare(boolean, boolean)` both order false first);
* `LivingEntity.addEffect` (a new effect is added; an existing one changes
  only when `update` says so);
* the periodic `shouldApplyEffectTickThisTick` of Regeneration, Poison and
  Wither, and `MobEffect`'s default (never);
* `Hud.extractEffects` (reverse natural order, `showIcon`, the blink alpha),
  `EffectsInInventory` (natural order, level text for amplifiers 1..9),
  `MobEffectUtil.formatDuration` and `StringUtil.formatTickDuration`;
* `EffectCommands`' `/effect give` argument tree and `computeDurationInTicks`.

Before generating anything the tool checks that every transcribed method
body still hashes to the text the model was written from (`PINNED`), and
reads the numbers it needs (colours, intervals, bounds, the compare cut-off,
the particle odds and alpha) from the source itself, so a different source
fails instead of producing a stale fixture. en_us strings come from the
pinned client's en_us.json. It reads no VoxelCraft code or fixture.
"""
import argparse
import functools
import hashlib
import json
import math
from pathlib import Path
import random
import re
import struct

# sha256 of each transcribed method body with all whitespace removed.
PINNED = {
    "MobEffectInstance.update": "57b0c3a59f49e2a386949740f0fc355512fbea660474d4cc1aca3459be20516b",
    "MobEffectInstance.isShorterDurationThan": "8740d9eefb44e9294ce52c58916c6705fe4cd7289dba6a2c42cf1c89b98b9ebe",
    "MobEffectInstance.isInfiniteDuration": "260445c520e82780dc61f91c9c3039365988ce2852088d0dfa5c54929b950c35",
    "MobEffectInstance.endsWithin": "5abd5ca91ff36f4812135cd17a7860b2569b2f5109bb24e578903160dc82e758",
    "MobEffectInstance.mapDuration": "135034dc78598e561a02642388da1f98750bbd0a86ddd828855f39dff508cfcd",
    "MobEffectInstance.tickServer": "e3dd46f0053fc3e0a39ffeb0d548d26936f04894bf5aec2e83e72c8a23982dc7",
    "MobEffectInstance.hasRemainingDuration": "488fc2d96e7f3e2641af760ff6789d881990acc617cc6a94545409fa02023ad8",
    "MobEffectInstance.tickDownDuration": "a378edd65901fb0c5c458efb8fb9be926336e4c1fce979458ee8f1659de6acb6",
    "MobEffectInstance.downgradeToHiddenEffect": "e7d8223e2259a28c7b19c693bf51466d91c117d3dc682105377fd0693bfd85bc",
    "MobEffectInstance.compareTo": "b04cc92a8682097ccd4bf612ba37601da05bdb004068bc492ee25cd3b16a80cc",
    "MobEffectInstance.setDetailsFrom": "5cd8060e20faf995f4a7ec371e066129baf9588fb02c49f147e834547e15ade7",
    "LivingEntity.addEffect": "efbadaf871b07a167c70299790946508b4510324ca24d0af493e008e33d23cd1",
    "LivingEntity.areAllEffectsAmbient": "a71971520e91c2217a5fa5672228277327f95c3a5d47c7c3d1bce90e7e831849",
    "LivingEntity.updateSynchronizedMobEffectParticles": "ba281ea2a8f12f0bbe4864fff715433abb7dcd7deeb8f0aca25a827d8674180b",
    "LivingEntity.tickEffects": "f5af2de5a84df0562c8558d996666001fba8fdb852c430229e88257bb02d3e04",
    "Hud.extractEffects": "c2f7edcea1d411fa81a08156cd7c6b8883fb56cbd890ec3262133c1adea57d06",
    "EffectsInInventory.getEffectName": "f188d1e07aa0d69a8568d53b176b0d9c21c3521c141122f5e6bd04a7f9185e37",
    "MobEffectUtil.formatDuration": "ab6cf01343eee1f0cb5b5d20b2463abe500b64d6d30565dde647fcfa650daa10",
    "StringUtil.formatTickDuration": "6c34b6e9f3211a339ba25fde8709a4b5190e187f2028724c3bea586199bba532",
    "EffectCommands.giveEffect": "acf220665b3ae8d083ed8828f6db0128bc8d8bffeac8343f9593a2d18a676a40",
    "EffectCommands.computeDurationInTicks": "6f853a8e74cddc6ac43e449306600704ed208463b10fae477e9d88e83eea0c02",
    "EffectCommands.register": "722e7889c487980b52d55399f89388b3ada37744e8da443b51bbe955382d9120",
    "MobEffect.shouldApplyEffectTickThisTick": "4bded6faed7c7b60188b2f13ae239474a0d0e0f8b9ee250dc25f2699666fdd68",
    "RegenerationMobEffect.shouldApplyEffectTickThisTick": "9061b9da21697fc9d53612ffde676f3d57baba4895fe6a621a143d7803ed8c99",
    "PoisonMobEffect.shouldApplyEffectTickThisTick": "e830ec48e5e9b050713e368f0192ecb29a5ded32b99fe10de6d8f3a562da189b",
    "WitherMobEffect.shouldApplyEffectTickThisTick": "733158848e3e0da6ae75844696c23df81d567248ba346632de4c86fd8cd3ba03",
    "Mth.cos": "bec6657e3a99510fc7374c5b28b0d58e4bc22e33dd7042ba63ce16b69de5ea8a",
}

FILES = {
    "MobEffectInstance": "world/effect/MobEffectInstance.java",
    "MobEffect": "world/effect/MobEffect.java",
    "MobEffects": "world/effect/MobEffects.java",
    "RegenerationMobEffect": "world/effect/RegenerationMobEffect.java",
    "PoisonMobEffect": "world/effect/PoisonMobEffect.java",
    "WitherMobEffect": "world/effect/WitherMobEffect.java",
    "LivingEntity": "world/entity/LivingEntity.java",
    "Hud": "client/gui/Hud.java",
    "EffectsInInventory": "client/gui/screens/inventory/EffectsInInventory.java",
    "MobEffectUtil": "world/effect/MobEffectUtil.java",
    "StringUtil": "util/StringUtil.java",
    "Mth": "util/Mth.java",
    "EffectCommands": "server/commands/EffectCommands.java",
}


# Overloaded methods: the start of the parameter list that picks the one modelled.
OVERLOADS = {"LivingEntity.addEffect": "final MobEffectInstance newEffect, @Nullable final Entity source"}


def method_body(text, name, params=""):
    """The `{...}` body of the first method declaration called `name` whose
    parameters start with `params`."""
    match = re.search(r"\n   (?:public |private |protected |static |final )*[\w<>\[\], .?]+ " + re.escape(name) + r"\("
                      + re.escape(params), text)
    if not match:
        raise ValueError(f"no method {name}")
    start = text.index("{", match.end())
    depth = 0
    for i in range(start, len(text)):
        depth += {"{": 1, "}": -1}.get(text[i], 0)
        if depth == 0:
            return text[start:i + 1]
    raise ValueError(f"unbalanced method {name}")


def digest(body):
    return hashlib.sha256(re.sub(r"\s+", "", body).encode()).hexdigest()


def need(pattern, text, what):
    match = re.search(pattern, text, re.S)
    if not match:
        raise ValueError(f"source no longer has {what}")
    return match


def f32(x):
    return struct.unpack("f", struct.pack("f", x))[0]


class Model:
    """The decompiled methods, transcribed. Durations are Java ints (-1 is infinite)."""

    def __init__(self, sources):
        self.src = sources
        inst = sources["MobEffectInstance"]
        self.infinite = int(need(r"INFINITE_DURATION = (-?\d+);", inst, "INFINITE_DURATION")[1])
        self.cutoff = int(need(r"int updateCutOff = (\d+);", inst, "compareTo cut-off")[1])
        effects = sources["MobEffects"]
        self.colour = {}
        self.cls = {}
        for id_, cls, _category, colour in re.findall(
                r'register\(\s*"(\w+)",\s*new (\w+)\(\s*MobEffectCategory\.(\w+),\s*(-?\d+)', effects):
            self.colour[id_] = int(colour)
            self.cls[id_] = cls
        if len(self.colour) != len(re.findall(r'register\(\s*"', effects)):
            raise ValueError("an effect registration has an unknown form")
        self.interval = {}
        for cls in ("RegenerationMobEffect", "PoisonMobEffect", "WitherMobEffect"):
            body = method_body(sources[cls], "shouldApplyEffectTickThisTick")
            self.interval[cls] = int(need(r"int interval = (\d+) >> amplification;", body, f"{cls} interval")[1])
            need(r"return interval > 0 \? tickCount % interval == 0 : true;", body, f"{cls} rule")
        need(r"return false;", method_body(sources["MobEffect"], "shouldApplyEffectTickThisTick"),
             "MobEffect's default")
        tick = method_body(sources["LivingEntity"], "tickEffects")
        self.bound = tuple(int(x) for x in need(r"int bound = this\.isInvisible\(\) \? (\d+) : (\d+);", tick,
                                                "particle odds").groups())
        self.ambient_factor = int(need(r"int ambientFactor = isAmbient \? (\d+) : 1;", tick, "ambient factor")[1])
        floor_of = float(need(r"AMBIENT_ALPHA = Mth\.floor\(([\d.]+)F\);", sources["MobEffect"], "AMBIENT_ALPHA")[1])
        self.ambient_alpha = math.floor(floor_of)
        need(r"int alpha = effectInstance\.isAmbient\(\) \? AMBIENT_ALPHA : 255;", sources["MobEffect"],
             "the particle alpha")
        hud = method_body(sources["Hud"], "extractEffects")
        self.blink = int(need(r"instance\.endsWithin\((\d+)\)", hud, "the HUD blink window")[1])
        need(r"Ordering\.natural\(\)\.reverse\(\)\.sortedCopy\(activeEffects\)", hud, "the HUD order")
        need(r"if \(instance\.showIcon\(\)\)", hud, "the HUD icon flag")
        need(r"sin\[i\] = \(float\)Math\.sin\(i / 10430\.378350470453\);", sources["Mth"], "Mth's SIN table")
        inv = method_body(sources["EffectsInInventory"], "getEffectName")
        self.level_range = tuple(int(x) for x in need(
            r"effect\.getAmplifier\(\) >= (\d+) && effect\.getAmplifier\(\) <= (\d+)", inv, "the level text range").groups())
        need(r"Ordering\.natural\(\)\.sortedCopy\(activeEffects\)", sources["EffectsInInventory"],
             "the inventory order")
        reg = method_body(sources["EffectCommands"], "register")
        self.seconds = tuple(int(x) for x in need(r'argument\("seconds", IntegerArgumentType\.integer\((\d+), (\d+)\)\)',
                                                 reg, "the seconds bounds").groups())
        self.amplifier = tuple(int(x) for x in need(
            r'argument\("amplifier", IntegerArgumentType\.integer\((\d+), (\d+)\)\)', reg, "the amplifier bounds").groups())
        need(r'Commands\.literal\("infinite"\)', reg, "the infinite literal")
        need(r'argument\("hideParticles", BoolArgumentType\.bool\(\)\)', reg, "hideParticles")
        compute = method_body(sources["EffectCommands"], "computeDurationInTicks")
        self.default_ticks = tuple(int(x) for x in need(r"return effect\.isInstantaneous\(\) \? (\d+) : (\d+);", compute,
                                                        "the default durations").groups())
        need(r"seconds \* (20)", compute, "seconds to ticks")
        self.instantaneous = {id_ for id_, cls in self.cls.items() if self.is_instantaneous(cls)}

    def is_instantaneous(self, cls):
        while cls not in ("MobEffect", "InstantaneousMobEffect"):
            path = Path(self.src["root"]) / "world/effect" / f"{cls}.java"
            cls = need(r"class " + cls + r" extends (\w+)", path.read_text(), f"{cls}'s superclass")[1]
        return cls == "InstantaneousMobEffect"

    # MobEffectInstance

    @staticmethod
    def new(effect, duration, amplifier, ambient=False, visible=True, show_icon=None):
        return {"effect": effect, "duration": duration, "amplifier": max(0, min(255, amplifier)), "ambient": ambient,
                "visible": visible, "show_icon": visible if show_icon is None else show_icon, "hidden": None}

    @staticmethod
    def copy(i):
        return dict(i, hidden=None)

    def is_infinite(self, i):
        return i["duration"] == self.infinite

    def shorter(self, a, b):
        return not self.is_infinite(a) and (a["duration"] < b["duration"] or self.is_infinite(b))

    def ends_within(self, i, ticks):
        return not self.is_infinite(i) and i["duration"] <= ticks

    def update(self, this, take):
        changed = False
        if take["amplifier"] > this["amplifier"]:
            if self.shorter(take, this):
                prev = this["hidden"]
                this["hidden"] = self.copy(this)
                this["hidden"]["hidden"] = prev
            this["amplifier"] = take["amplifier"]
            this["duration"] = take["duration"]
            changed = True
        elif self.shorter(this, take):
            if take["amplifier"] == this["amplifier"]:
                this["duration"] = take["duration"]
                changed = True
            elif this["hidden"] is None:
                this["hidden"] = self.copy(take)
            else:
                self.update(this["hidden"], take)
        if not take["ambient"] and this["ambient"] or changed:
            this["ambient"] = take["ambient"]
            changed = True
        if take["visible"] != this["visible"]:
            this["visible"] = take["visible"]
            changed = True
        if take["show_icon"] != this["show_icon"]:
            this["show_icon"] = take["show_icon"]
            changed = True
        return changed

    def add_effect(self, active, new):
        """`LivingEntity.addEffect` on a map of effect id to instance."""
        if new["effect"] not in active:
            active[new["effect"]] = new
            return True
        return self.update(active[new["effect"]], new)

    def should_apply(self, effect, tick_count, amplifier):
        cls = self.cls[effect]
        if cls in self.interval:
            interval = self.interval[cls] >> amplifier
            return tick_count % interval == 0 if interval > 0 else True
        return False

    def map_duration(self, i, fn):
        return fn(i["duration"]) if not self.is_infinite(i) and i["duration"] != 0 else i["duration"]

    def tick_down(self, i):
        if i["hidden"] is not None:
            self.tick_down(i["hidden"])
        i["duration"] = self.map_duration(i, lambda d: d - 1)

    def downgrade(self, i):
        if i["duration"] == 0 and i["hidden"] is not None:
            hidden = i["hidden"]
            for key in ("duration", "amplifier", "ambient", "visible", "show_icon"):
                i[key] = hidden[key]
            i["hidden"] = hidden["hidden"]
            return True
        return False

    def tick_server(self, i, tick_count):
        """(still active, applied this tick); the effect's tick never fails here."""
        if not (self.is_infinite(i) or i["duration"] > 0):
            return False, False
        clock = tick_count if self.is_infinite(i) else i["duration"]
        applied = self.should_apply(i["effect"], clock, i["amplifier"])
        self.tick_down(i)
        self.downgrade(i)
        return self.is_infinite(i) or i["duration"] > 0, applied

    def compare(self, a, b):
        def chain(*pairs):
            for x, y in pairs:
                if x != y:
                    return -1 if x < y else 1
            return 0

        if (a["duration"] <= self.cutoff or b["duration"] <= self.cutoff) and (not a["ambient"] or not b["ambient"]):
            return chain((a["ambient"], b["ambient"]), (self.is_infinite(a), self.is_infinite(b)),
                         (a["duration"], b["duration"]), (self.colour[a["effect"]], self.colour[b["effect"]]))
        return chain((a["ambient"], b["ambient"]), (self.colour[a["effect"]], self.colour[b["effect"]]))

    # Screens

    @staticmethod
    def mth_cos(x):
        """`Mth.cos`: the 65536-entry `SIN` table, a quarter turn on."""
        scale = 10430.378350470453
        return f32(math.sin((math.trunc(x * scale + 16384.0) & 65535) / scale))

    def hud_alpha(self, i):
        """`Hud.extractEffects`' icon alpha, in single precision."""
        if i["ambient"] or not self.ends_within(i, self.blink):
            return 1.0
        remaining = i["duration"]
        used = 10 - int(remaining / 20)
        fade = min(max(f32(f32(f32(remaining / 10.0) / 5.0) * 0.5), 0.0), 0.5)
        blink = self.mth_cos(f32(f32(remaining * f32(math.pi)) / 5.0))
        depth = min(max(f32(f32(used / 10.0) * 0.25), 0.0), 0.25)
        return min(max(f32(fade + f32(blink * depth)), 0.0), 1.0)

    def format_duration(self, i, lang):
        if self.is_infinite(i):
            return lang["effect.duration.infinite"]
        seconds = math.floor(i["duration"] / 20.0)
        minutes, seconds = seconds // 60, seconds % 60
        hours, minutes = minutes // 60, minutes % 60
        return f"{hours:02d}:{minutes:02d}:{seconds:02d}" if hours > 0 else f"{minutes:02d}:{seconds:02d}"

    # /effect give <targets> <effect> ...

    def parse_give(self, effect, args):
        def read_int(word, lo, hi):
            if not word or any(c not in "0123456789.-" for c in word):
                return None
            try:
                value = int(word)
            except ValueError:
                return None
            return value if lo <= value <= hi and -2**31 <= value < 2**31 else None

        if len(args) > 3:
            return None
        if args and args[0] == "infinite":
            seconds = -1
        elif args:
            seconds = read_int(args[0], *self.seconds)
            if seconds is None:
                return None
        else:
            seconds = None
        amplifier = 0
        if len(args) > 1:
            amplifier = read_int(args[1], *self.amplifier)
            if amplifier is None:
                return None
        particles = True
        if len(args) > 2:
            if args[2] not in ("true", "false"):
                return None
            particles = args[2] != "true"
        if seconds is None:
            ticks = self.default_ticks[0] if effect in self.instantaneous else self.default_ticks[1]
        elif effect in self.instantaneous:
            ticks = seconds
        else:
            ticks = -1 if seconds == -1 else seconds * 20
        return {"ticks": ticks, "amplifier": amplifier, "particles": particles}


def chain_of(model, i):
    out = []
    while i is not None:
        out.append([i["duration"], i["amplifier"], i["ambient"], i["visible"], i["show_icon"]])
        i = i["hidden"]
    return out


def generate(model, lang):
    rng = random.Random(2611)
    durations = [-1, 1, 20, 100, 199, 200, 600, 32147, 32148, 40000]
    kept = sorted(e for e in model.colour if e not in ("instant_health", "instant_damage"))

    def instance(effect):
        visible = rng.random() < 0.7
        return model.new(effect, rng.choice(durations), rng.randrange(4), rng.random() < 0.3, visible,
                         visible if rng.random() < 0.7 else not visible)

    def spec(i):
        return [i["duration"], i["amplifier"], i["ambient"], i["visible"], i["show_icon"]]

    merges = []
    named = [
        [(600, 0), (100, 1)], [(100, 1), (600, 0)], [(-1, 0), (600, 1)], [(600, 1), (-1, 0)], [(-1, 0), (-1, 0)],
        [(600, 0), (-1, 0)], [(-1, 2), (100, 1), (600, 0)], [(600, 2), (900, 1), (1200, 0), (700, 3)],
    ]
    for adds in named:
        merges.append([model.new("speed", d, a) for d, a in adds])
    for flags in [(True, True, True), (False, False, True), (False, False, False), (True, False, False)]:
        merges.append([model.new("speed", 600, 0, *flags), model.new("speed", 100, 0)])
        merges.append([model.new("speed", 600, 0), model.new("speed", 100, 0, *flags)])
        merges.append([model.new("speed", 600, 1, *flags), model.new("speed", 6000, 0)])
    while len(merges) < 300:
        effect = rng.choice(["speed", "regeneration", "slowness"])
        merges.append([instance(effect) for _ in range(rng.randrange(2, 8))])
    merge_cases = []
    for adds in merges:
        active = {}
        changed = [model.add_effect(active, dict(i)) for i in adds]
        merge_cases.append({"effect": adds[0]["effect"], "adds": [spec(i) for i in adds], "changed": changed,
                            "chain": chain_of(model, active[adds[0]["effect"]])})

    tick_cases = []
    for k in range(60):
        effect = ["regeneration", "poison", "wither", "speed"][k % 4]
        adds = [model.new(effect, rng.choice([-1, 30, 45, 90, 130]), rng.randrange(3)) for _ in range(1 + k % 3)]
        start, ticks = rng.randrange(0, 50), rng.choice([40, 90, 160])
        active = {}
        for i in adds:
            model.add_effect(active, dict(i))
        tick_count, applied, remaining = start, [], True
        for t in range(1, ticks + 1):
            tick_count += 1
            remaining, hit = model.tick_server(active[effect], tick_count)
            if hit:
                applied.append(t)
            if not remaining:
                break
        tick_cases.append({"effect": effect, "adds": [spec(i) for i in adds], "age": start, "ticks": ticks,
                           "applied": applied, "chain": chain_of(model, active[effect]) if remaining else []})

    order_cases = []
    for _ in range(150):
        chosen = rng.sample(kept, rng.randrange(2, 7))
        group = [model.new(e, rng.choice(durations), 0, rng.random() < 0.3) for e in chosen]
        natural = sorted(group, key=functools.cmp_to_key(model.compare))
        reverse = sorted(group, key=functools.cmp_to_key(lambda a, b: model.compare(b, a)))
        order_cases.append({"effects": [[i["effect"], i["duration"], i["ambient"]] for i in group],
                            "natural": [i["effect"] for i in natural], "reverse": [i["effect"] for i in reverse]})

    alpha_cases = [{"duration": d, "ambient": a, "alpha": model.hud_alpha(model.new("speed", d, 0, a))}
                   for d in [-1, 0, 1, 2, 5, 7, 10, 19, 20, 33, 50, 99, 100, 150, 199, 200, 201, 600] for a in (False, True)]
    duration_cases = [{"duration": d, "text": model.format_duration(model.new("speed", d, 0), lang)}
                      for d in [-1, 0, 19, 20, 600, 3700, 71999, 72000, 72020, 1_000_000 * 20]]
    level_cases = [{"amplifier": a, "level_text": model.level_range[0] <= a <= model.level_range[1]}
                   for a in [0, 1, 2, 8, 9, 10, 255]]

    command_args = [[], ["30"], ["1"], ["0"], ["1000000"], ["1000001"], ["-1"], ["infinite"], ["Infinite"], ["+5"],
                    ["05"], ["1.5"], ["10", "0"], ["10", "255"], ["10", "256"], ["10", "-1"], ["10", "+1"],
                    ["10", "1", "true"], ["10", "1", "false"], ["10", "1", "TRUE"], ["10", "1", "yes"],
                    ["infinite", "2"], ["infinite", "2", "true"], ["infinite", "2", "false", "x"], ["10", "1", "true", "x"]]
    command_cases = []
    for effect in ("speed", "saturation", "instant_health"):
        for args in command_args:
            parsed = model.parse_give(effect, args)
            command_cases.append({"effect": effect, "args": args, "result": parsed})

    return {
        "infinite_duration": model.infinite,
        "compare_cutoff": model.cutoff,
        "ambient_particle_alpha": model.ambient_alpha,
        "particle_bound": {"invisible": model.bound[0], "visible": model.bound[1]},
        "all_ambient_factor": model.ambient_factor,
        "hud_blink_ticks": model.blink,
        "periodic_intervals": {cls[:-len("MobEffect")].lower(): n for cls, n in sorted(model.interval.items())},
        "instance_fields": ["duration", "amplifier", "ambient", "show_particles", "show_icon"],
        "merge_cases": merge_cases,
        "tick_cases": tick_cases,
        "order_cases": order_cases,
        "hud_alpha_cases": alpha_cases,
        "duration_text_cases": duration_cases,
        "inventory_level_cases": level_cases,
        "command": {"seconds": list(model.seconds), "amplifier": list(model.amplifier),
                    "default_ticks": {"instantaneous": model.default_ticks[0], "other": model.default_ticks[1]},
                    "cases": command_cases},
        "feedback": {k: v for k, v in sorted(lang.items()) if k.startswith("commands.effect.")},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="decompiled Java 26.3 source: the net/minecraft folder")
    parser.add_argument("--en-us", type=Path, required=True, help="en_us.json from the pinned Java 26.3 client")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--print-pins", action="store_true", help="print the method hashes instead of checking them")
    args = parser.parse_args()
    sources = {name: (args.source / path).read_text() for name, path in FILES.items()}
    sources["root"] = str(args.source)
    hashes = {key: digest(method_body(sources[key.split(".")[0]], key.split(".")[1], OVERLOADS.get(key, "")))
              for key in PINNED}
    if args.print_pins:
        print(json.dumps(hashes, indent=4))
        return
    changed = [key for key, value in hashes.items() if PINNED[key] != value]
    if changed:
        raise SystemExit("the model was written from different source; re-check: " + ", ".join(changed))
    lang = json.loads(args.en_us.read_text())
    model = Model(sources)
    fixture = {
        "reference": "Minecraft Java Edition 26.3",
        "sources": sorted(PINNED) + ["MobEffects registrations (colour, class)",
                                     "en_us.json commands.effect.* and effect.duration.infinite"],
        **generate(model, lang),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(fixture, indent=1, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
