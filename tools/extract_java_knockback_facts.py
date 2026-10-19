"""Sweep attack and knockback facts for `extract_java_combat_fixtures.py`.

Each fact is read from the pinned Java 26.3 bytecode (`javap -c -p`) or data
files, and every method is checked against the exact call and constant sequence
it was read from, so a changed form fails instead of regenerating guessed data.
The reference `cases` are then computed here from the extracted constants with a
small independent model of Java's arithmetic (float rounding where Java uses
`float`); they never read VoxelCraft source or an existing fixture.
"""
import json
import math
import re
import struct
import zipfile

PLAYER = "net.minecraft.world.entity.player.Player"
LIVING = "net.minecraft.world.entity.LivingEntity"
ENTITY = "net.minecraft.world.entity.Entity"
MOB = "net.minecraft.world.entity.Mob"
ARROW = "net.minecraft.world.entity.projectile.arrow.AbstractArrow"
PROJECTILE = "net.minecraft.world.entity.projectile.Projectile"
MTH = "net.minecraft.util.Mth"
SOURCE = "net.minecraft.world.damagesource.DamageSource"


def f32(value):
    return struct.unpack("f", struct.pack("f", value))[0]


def body(code, signature):
    parts = code.split("  " + signature)
    if len(parts) != 2:
        raise ValueError(f"missing reference method: {signature}")
    return parts[1].split("\n\n")[0]


def refs(code):
    """Calls and field accesses in order, as `Class.member` (or bare `member`)."""
    found = []
    for match in re.finditer(r"//\s+(?:Method|InterfaceMethod|Field)\s+([^:\s]+):", code):
        found.append(match[1].split("/")[-1])
    return found


def mnemonics(code):
    return [m[1] for m in re.finditer(r"^\s+\d+: (\w+)", code, re.M)]


def numbers(code):
    """Numeric constants in order: ints, longs, floats and doubles."""
    values = []
    for line in code.splitlines():
        match = re.search(r"\d+: (?:(iconst_m1|iconst_[0-5]|lconst_[01]|fconst_[0-2]|dconst_[01])"
                          r"|(?:bipush|sipush)\s+(-?\d+)"
                          r"|ldc(?:2_w|_w)?\s+#\d+\s+// (?:float|double|int|long) (-?[\d.E+-]+)[fdl]?)\s*$", line)
        if not match:
            continue
        if match[1]:
            values.append(-1.0 if match[1].endswith("m1") else float(match[1][-1]))
        else:
            values.append(float(match[2] if match[2] is not None else match[3]))
    return values


def require(condition, what):
    if not condition:
        raise ValueError(f"unrecognized bytecode: {what}")


def window(code, patterns):
    """Whether instructions matching `patterns` (substring of `mnemonic operand`) occur contiguously."""
    tokens = []
    for line in code.splitlines():
        match = re.match(r"\s+\d+: (\w+)\s*(.*)$", line)
        if match:
            tokens.append(f"{match[1]} {match[2]}")
    width = len(patterns)
    return any(all(pattern in token for pattern, token in zip(patterns, tokens[i:i + width]))
               for i in range(len(tokens) - width + 1))


def has_refs(code, expected, what):
    """The call/field sequence must contain `expected` in order (other refs may interleave)."""
    found = iter(name.replace('"', '').split(".")[-1] for name in refs(code))
    require(all(any(item.split(".")[-1] == name for name in found) for item in expected),
            f"{what}: {expected} in {refs(code)}")


def ranged_attribute(registry, name):
    found = re.search(r"// String " + re.escape(name) + r"\n((?:[^\n]*\n){3})", registry)
    require(found, f"attribute range {name}")
    values = []
    for line in found[1].splitlines():
        constant = re.search(r"dconst_([01])|double (-?[\d.]+)d", line)
        require(constant, f"attribute range {name}: {line}")
        values.append(float(constant[1] if constant[1] is not None else constant[2]))
    return dict(zip(("default", "min", "max"), values))


def mth_tables():
    table = [f32(math.sin(i / 10430.378350470453)) for i in range(65536)]

    def sin(x):
        return table[int(x * 10430.378350470453) & 65535]

    def cos(x):
        return table[int(x * 10430.378350470453 + 16384.0) & 65535]

    return sin, cos


def extract_knockback(inspect, jar_path):
    player = inspect(PLAYER, constants=True)
    living = inspect(LIVING, constants=True)
    entity = inspect(ENTITY)
    mob = inspect(MOB)
    arrow = inspect(ARROW, constants=True)
    projectile = inspect(PROJECTILE)
    source = inspect(SOURCE)
    mth = inspect(MTH, constants=True)
    registry = inspect("net.minecraft.world.entity.ai.attributes.Attributes")

    # --- Mth.sin / Mth.cos lookup tables -------------------------------------------------
    sin_code = body(mth, "public static float sin(double);")
    cos_code = body(mth, "public static float cos(double);")
    require(numbers(sin_code) == [10430.378350470453, 65535.0], "Mth.sin")
    require(numbers(cos_code) == [10430.378350470453, 16384.0, 65535.0], "Mth.cos")
    require(mnemonics(sin_code)[3:6] == ["dmul", "d2l", "ldc2_w"] and "land" in mnemonics(sin_code), "Mth.sin shape")
    table_init = body(mth, "private static void lambda$static$0(float[]);")
    require(numbers(table_init) == [0.0, 10430.378350470453] and "ddiv" in mnemonics(table_init)
            and "Math.sin" in refs(table_init), "Mth.SIN table initialisation")
    sin, cos = mth_tables()

    # --- Attribute ranges ------------------------------------------------------------------
    attributes = {name: ranged_attribute(registry, f"attribute.name.{name}")
                  for name in ("attack_knockback", "knockback_resistance", "sweeping_damage_ratio")}
    require(attributes["attack_knockback"] == {"default": 0.0, "min": 0.0, "max": 5.0}, "attack_knockback range")
    require(attributes["knockback_resistance"] == {"default": 0.0, "min": -2.0, "max": 1.0}, "resistance range")
    require(attributes["sweeping_damage_ratio"] == {"default": 0.0, "min": 0.0, "max": 1.0}, "sweeping range")
    movement = re.search(r"String attribute.name.movement_speed\n[^\n]*double ([\d.]+)d\n[^\n]*(dconst_0)\n[^\n]*double ([\d.]+)d",
                         registry)
    require(movement, "movement speed range")
    create = body(player, "public static net.minecraft.world.entity.ai.attributes.AttributeSupplier$Builder createAttributes();")
    walking = re.search(r"Attributes\.MOVEMENT_SPEED:[^\n]*\n[^\n]*double ([\d.]+)d", create)
    require(walking, "player movement speed")
    player_speed = float(walking[1])
    require(abs(player_speed - 0.1) < 1e-8, "player base speed is 0.1f")

    # --- LivingEntity.knockback(strength, x, z, source, damage, flag) ---------------------
    knockback = body(living, "public void knockback(double, double, double, net.minecraft.world.damagesource.DamageSource, float, boolean);")
    has_refs(knockback, ["Attributes.KNOCKBACK_RESISTANCE", "getAttributeValue", "needsSync", "getDeltaMovement",
                         "random", "nextDouble", "nextDouble", "random", "nextDouble", "nextDouble", "Vec3.normalize",
                         "Vec3.scale", "onGround", "Math.min", "setDeltaMovement"], "LivingEntity.knockback")
    require(numbers(knockback) == [1.0, 0.0, 1.0, 9.999999747378752e-06, 0.01, 0.01, 0.0, 2.0, 0.4, 2.0, 2.0],
            f"LivingEntity.knockback constants {numbers(knockback)}")
    require("flag" not in knockback and re.search(r"\biload\s+7\b|\biload 7\b", knockback) is None, "knockback flag unused")
    # strength *= 1 - resistance; return when <= 0; jitter while x*x + z*z < 1e-5; direction = normalize(x, 0, z) * strength;
    # x' = x0 / 2 - dir.x; y' = onGround ? min(0.4, y0 / 2 + strength) : y0; z' = z0 / 2 - dir.z.
    require(mnemonics(knockback)[:12] == ["dload_1", "dconst_1", "aload_0", "getstatic", "invokevirtual", "dsub", "dmul",
                                          "dstore_1", "dload_1", "dconst_0", "dcmpg", "ifgt"], "resistance gate shape")
    require(re.search(r"dload_3\n[^\n]*dload_3\n[^\n]*dmul\n[^\n]*dload\s+5\n[^\n]*dload\s+5\n[^\n]*dmul\n[^\n]*dadd\n"
                      r"[^\n]*9\.999999747378752E-6d\n[^\n]*dcmpg\n[^\n]*ifge", knockback), "squared length test")
    require(re.search(r"dload_3\n[^\n]*dconst_0\n[^\n]*dload\s+5\n[^\n]*invokespecial[^\n]*Vec3.\"<init>\"", knockback)
            and "Vec3.normalize" in knockback, "direction is normalize(x, 0, z)")
    require(re.search(r"Vec3\.x:D\n[^\n]*ldc2_w[^\n]*2\.0d\n[^\n]*ddiv\n[^\n]*aload\s+11\n[^\n]*Vec3\.x:D\n[^\n]*dsub", knockback)
            and re.search(r"ldc2_w[^\n]*0\.4d\n[^\n]*aload\s+10\n[^\n]*Vec3\.y:D\n[^\n]*ldc2_w[^\n]*2\.0d\n[^\n]*ddiv\n"
                          r"[^\n]*dload_1\n[^\n]*dadd\n[^\n]*Math.min", knockback)
            and re.search(r"Vec3\.z:D\n[^\n]*ldc2_w[^\n]*2\.0d\n[^\n]*ddiv\n[^\n]*aload\s+11\n[^\n]*Vec3\.z:D\n[^\n]*dsub", knockback),
            "new motion = old / 2 - push horizontally; vertical min(0.4, old / 2 + strength) on the ground")
    short = body(living, "public void knockback(double, double, double, net.minecraft.world.damagesource.DamageSource, float);")
    require(refs(short)[-1] == "knockback" and numbers(short) == [0.0], "five argument knockback passes false")

    # --- LivingEntity.getKnockback / dealDefaultKnockback ---------------------------------
    get_knockback = body(living, "protected float getKnockback(net.minecraft.world.entity.Entity, net.minecraft.world.damagesource.DamageSource);")
    has_refs(get_knockback, ["Attributes.ATTACK_KNOCKBACK", "getAttributeValue", "EnchantmentHelper.modifyKnockback"], "getKnockback")
    require(numbers(get_knockback) == [2.0, 2.0] and mnemonics(get_knockback).count("fdiv") == 2, "getKnockback halves")
    default_constant = re.search(r"static final float DEFAULT_KNOCKBACK = ([\d.]+)f;", living)
    require(default_constant and float(default_constant[1]) == 0.4, "DEFAULT_KNOCKBACK")
    deal = body(living, "public void dealDefaultKnockback(net.minecraft.world.damagesource.DamageSource, float, boolean);")
    has_refs(deal, ["DamageSource.getDirectEntity", "Projectile.calculateHorizontalHurtKnockbackDirection",
                    "leftDouble", "rightDouble", "DamageSource.getSourcePosition", "Vec3.x", "getX", "Vec3.z", "getZ",
                    "knockback", "indicateDamage"], "dealDefaultKnockback")
    require(numbers(deal) == [0.0, 0.0, 0.4000000059604645], f"dealDefaultKnockback constants {numbers(deal)}")
    require(mnemonics(deal).count("dneg") == 2 and mnemonics(deal).count("dsub") == 2, "projectile direction is negated")
    hurt_server = body(living, "public boolean hurtServer(net.minecraft.server.level.ServerLevel, net.minecraft.world.damagesource.DamageSource, float);")
    has_refs(hurt_server, ["DamageTypeTags.NO_KNOCKBACK", "DamageSource.is", "dealDefaultKnockback"], "hurtServer knockback gate")
    cooldown = re.search(r"damageCooldownTime:I\n\s+\d+: i2f\n\s+\d+: ldc\w*\s+#\d+\s+// float ([\d.]+)f", hurt_server)
    require(cooldown and float(cooldown[1]) == 10.0, "hurt cooldown threshold")
    projectile_direction = body(projectile, "public it.unimi.dsi.fastutil.doubles.DoubleDoubleImmutablePair "
                                            "calculateHorizontalHurtKnockbackDirection(net.minecraft.world.entity.LivingEntity, "
                                            "net.minecraft.world.damagesource.DamageSource);")
    has_refs(projectile_direction, ["getDeltaMovement", "Vec3.x", "getDeltaMovement", "Vec3.z", "DoubleDoubleImmutablePair.of"],
             "projectile hurt direction")
    source_position = body(source, "public net.minecraft.world.phys.Vec3 getSourcePosition();")
    has_refs(source_position, ["damageSourcePosition", "damageSourcePosition", "directEntity", "directEntity", "Entity.position"],
             "DamageSource.getSourcePosition")

    # --- Player.attack knockback plumbing --------------------------------------------------
    attack = body(player, "public void attack(net.minecraft.world.entity.Entity);")
    has_refs(attack, ["isSprinting", "SoundEvents.PLAYER_ATTACK_KNOCKBACK", "playServerSideSound"], "sprint knockback sound")
    has_refs(attack, ["hurtOrSimulate", "getKnockback", "causeExtraKnockback", "doSweepAttack", "attackVisualEffects",
                      "setLastHurtMob", "itemAttackInteraction", "damageStatsAndHearts", "causeFoodExhaustion",
                      "SoundEvents.PLAYER_ATTACK_NODAMAGE"], "Player.attack order")
    require([v for v in numbers(attack) if v not in (0.0, 1.0)] == [0.5, 0.9, 1.5, 0.5, 0.1], "Player.attack constants")
    extra = body(player, "public void causeExtraKnockback(net.minecraft.world.entity.Entity, float, net.minecraft.world.phys.Vec3, "
                         "net.minecraft.world.damagesource.DamageSource, float, boolean);")
    has_refs(extra, ["getYRot", "Mth.sin", "getYRot", "Mth.cos", "knockback", "getYRot", "Mth.sin", "getYRot", "Mth.cos",
                     "push", "getDeltaMovement", "Vec3.multiply", "setDeltaMovement", "setSprinting",
                     "syncVelocity", "connection", "<init>", "send", "syncVelocity", "setDeltaMovement"],
             "Player.causeExtraKnockback")
    require(numbers(extra) == [0.0, 0.017453292, 0.017453292, 0.017453292, 0.1, 0.017453292, 0.6, 1.0, 0.6, 0.0, 0.0],
            f"Player.causeExtraKnockback constants {numbers(extra)}")
    require("iconst_0" in mnemonics(extra), "sprint cleared")
    living_extra = body(living, "public void causeExtraKnockback(net.minecraft.world.entity.Entity, float, net.minecraft.world.phys.Vec3, "
                                "net.minecraft.world.damagesource.DamageSource, float, boolean);")
    has_refs(living_extra, ["getYRot", "Mth.sin", "getYRot", "Mth.cos", "knockback", "getDeltaMovement", "Vec3.multiply",
                            "setDeltaMovement"], "LivingEntity.causeExtraKnockback")
    require(numbers(living_extra) == [0.0, 0.017453292, 0.017453292, 0.6, 1.0, 0.6], "LivingEntity.causeExtraKnockback constants")
    # Mob.doHurtTarget: hurt, then getKnockback via causeExtraKnockback with the pre-hit motion, then the item/enchantment hooks.
    do_hurt = body(mob, "public boolean doHurtTarget(net.minecraft.server.level.ServerLevel, net.minecraft.world.entity.Entity);")
    has_refs(do_hurt, ["Attributes.ATTACK_DAMAGE", "getAttributeValue", "EnchantmentHelper.modifyDamage", "getAttackDamageBonus",
                       "getDeltaMovement", "hurtServer", "getKnockback", "causeExtraKnockback", "hurtEnemy",
                       "EnchantmentHelper.doPostAttackEffects", "setLastHurtMob", "playAttackSound"], "Mob.doHurtTarget")
    # --- Hoglin and zoglin tusk toss (HoglinBase) --------------------------------------------
    hoglin_base = inspect("net.minecraft.world.entity.monster.hoglin.HoglinBase", constants=True)
    throw = body(hoglin_base, "public static void throwTarget(net.minecraft.world.entity.LivingEntity, net.minecraft.world.entity.LivingEntity);")
    has_refs(throw, ["Attributes.ATTACK_KNOCKBACK", "getAttributeValue", "Attributes.KNOCKBACK_RESISTANCE", "getAttributeValue",
                     "getX", "getX", "getZ", "getZ", "getRandom", "nextInt", "nextFloat", "Vec3.normalize", "Vec3.scale", "Vec3.yRot",
                     "nextFloat", "push"], "HoglinBase.throwTarget")
    require(numbers(throw) == [0.0, 21.0, 10.0, 0.5, 0.2, 0.0, 0.5, 1.0], f"throwTarget constants {numbers(throw)}")
    require(window(throw, ["dload", "dload", "dsub", "dstore"]) and window(throw, ["dload", "dconst_0", "dcmpg", "ifgt"]),
            "toss strength is attack_knockback minus resistance, cancelled at or below zero")
    require(window(throw, ["invokeinterface", "bipush", "isub", "i2f", "fstore"]),
            "turn is nextInt(21) - 10 converted to float, passed to yRot without a degree conversion")
    hurt_throw = body(hoglin_base, "public static boolean hurtAndThrowTarget(net.minecraft.server.level.ServerLevel, "
                                   "net.minecraft.world.entity.LivingEntity, net.minecraft.world.entity.LivingEntity);")
    has_refs(hurt_throw, ["hurtServer", "doPostAttackEffects", "isBaby", "throwTarget"], "HoglinBase.hurtAndThrowTarget")
    for cls in ("net.minecraft.world.entity.monster.hoglin.Hoglin", "net.minecraft.world.entity.monster.Zoglin"):
        do_hurt_target = body(inspect(cls), "public boolean doHurtTarget(net.minecraft.server.level.ServerLevel, net.minecraft.world.entity.Entity);")
        require("causeExtraKnockback" not in do_hurt_target and "getKnockback" not in do_hurt_target, f"{cls} uses no extra knockback")
    vec3 = inspect("net.minecraft.world.phys.Vec3")
    y_rot = body(vec3, "public net.minecraft.world.phys.Vec3 yRot(float);")
    has_refs(y_rot, ["Mth.cos", "Mth.sin", "Vec3.x", "Vec3.z", "Vec3.z", "Vec3.x"], "Vec3.yRot")
    require(window(y_rot, ["fload_1", "f2d", "invokestatic", "fstore_2"]), "yRot takes radians")

    # --- Vec3.normalize: a zero vector below a length of 1.0E-5f ----------------------------
    normalize = body(vec3, "public net.minecraft.world.phys.Vec3 normalize();")
    require(numbers(normalize) == [9.999999747378752e-06] and "dcmpg" in mnemonics(normalize)
            and "Math.sqrt" in refs(normalize) and "ZERO" in " ".join(refs(normalize)),
            f"Vec3.normalize cutoff {numbers(normalize)} {refs(normalize)}")

    # --- Player.getSpeed -------------------------------------------------------------------
    speed = body(player, "public float getSpeed();")
    has_refs(speed, ["Attributes.MOVEMENT_SPEED", "getAttributeValue"], "Player.getSpeed")
    # --- Arrow knockback -------------------------------------------------------------------
    arrow_knockback = body(arrow, "protected void doKnockback(net.minecraft.world.entity.LivingEntity, net.minecraft.world.damagesource.DamageSource);")
    has_refs(arrow_knockback, ["firedFromWeapon", "EnchantmentHelper.modifyKnockback", "Attributes.KNOCKBACK_RESISTANCE",
                               "getAttributeValue", "Math.max", "getDeltaMovement", "Vec3.multiply", "Vec3.normalize",
                               "Vec3.scale", "Vec3.lengthSqr", "push"], "AbstractArrow.doKnockback")
    require(numbers(arrow_knockback) == [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.6, 0.0, 0.1],
            f"doKnockback constants {numbers(arrow_knockback)}")

    # --- Knockback enchantments and the sweeping attribute ---------------------------------
    with zipfile.ZipFile(jar_path) as inner:
        def data(name):
            return json.loads(inner.read(f"data/minecraft/{name}.json"))

        knockback_enchant = data("enchantment/knockback")
        punch_enchant = data("enchantment/punch")
        sweeping_enchant = data("enchantment/sweeping_edge")
        swords = data("tags/item/swords")["values"]
        arrows = data("tags/entity_type/arrows")["values"]
    for enchant in (knockback_enchant, punch_enchant):
        effect = enchant["effects"]["minecraft:knockback"][0]["effect"]
        require(effect["type"] == "minecraft:add" and effect["value"] == {"type": "minecraft:linear", "base": 1.0,
                                                                          "per_level_above_first": 1.0},
                "knockback enchantment value")
    require(punch_enchant["effects"]["minecraft:knockback"][0]["requirements"]["predicate"]["minecraft:entity_type"] == "#minecraft:arrows",
            "punch applies to arrow direct attackers")
    require("requirements" not in knockback_enchant["effects"]["minecraft:knockback"][0], "knockback has no requirement")
    modifier = sweeping_enchant["effects"]["minecraft:attributes"][0]
    require(modifier["attribute"] == "minecraft:sweeping_damage_ratio" and modifier["operation"] == "add_value"
            and modifier["amount"]["type"] == "minecraft:fraction", "sweeping edge attribute modifier")

    def linear(spec, level):
        return spec["base"] + spec["per_level_above_first"] * (level - 1)

    sweeping_ratio = {str(level): linear(modifier["amount"]["numerator"], level) / linear(modifier["amount"]["denominator"], level)
                      for level in range(1, sweeping_enchant["max_level"] + 1)}

    # --- Attributes the implemented mobs set ------------------------------------------------
    mob_attributes = {}
    for cls, short_name in [("net.minecraft.world.entity.animal.golem.IronGolem", "iron_golem"),
                            ("net.minecraft.world.entity.monster.hoglin.Hoglin", "hoglin"),
                            ("net.minecraft.world.entity.monster.Zoglin", "zoglin"),
                            ("net.minecraft.world.entity.monster.Ravager", "ravager"),
                            ("net.minecraft.world.entity.monster.warden.Warden", "warden")]:
        text = inspect(cls)
        created = re.search(r"static net\.minecraft\.world\.entity\.ai\.attributes\.AttributeSupplier\$Builder "
                            r"\w*[Aa]ttributes\(\);\n    Code:\n(.*?)\n\n", text, re.S)
        require(created, f"{cls} attributes")
        facts = {}
        for attribute, value, const in re.findall(
                r"Attributes\.(KNOCKBACK_RESISTANCE|ATTACK_KNOCKBACK):[^\n]*\n[^\n]*?(?:ldc2_w[^\n]*double ([\d.]+)d|(dconst_[01]))",
                created[1]):
            facts[attribute.lower()] = float(value) if value else float(const[-1])
        mob_attributes[short_name] = facts
    require(mob_attributes["iron_golem"] == {"knockback_resistance": 1.0}, "iron golem")
    require(mob_attributes["hoglin"] == {"knockback_resistance": 0.6000000238418579, "attack_knockback": 1.0}, "hoglin")
    require(mob_attributes["zoglin"] == {"knockback_resistance": 0.6000000238418579, "attack_knockback": 1.0}, "zoglin")

    # --- Effects that change the player's movement speed ------------------------------------
    effects = inspect("net.minecraft.world.effect.MobEffects").split("  static {};")[1]
    speed_effects = {}
    previous = 0
    for end in re.finditer(r"putstatic\s+#\d+\s+// Field ([A-Z_]+):Lnet/minecraft/core/Holder;", effects):
        code = effects[previous:end.start()]
        previous = end.end()
        found = re.search(r"Attributes\.MOVEMENT_SPEED:[^\n]*\n[^\n]*\n[^\n]*\n[^\n]*double (-?[\d.]+)d\n[^\n]*Operation\.(\w+):", code)
        if found and end[1] in ("SPEED", "SLOWNESS"):
            speed_effects[end[1].lower()] = {"amount_per_level": float(found[1]), "operation": found[2].lower()}
    require(set(speed_effects) == {"speed", "slowness"}, f"movement speed effects {speed_effects}")
    static_init = living.split("  static {};")[1]
    sprint = re.search(r"Field SPRINTING_MODIFIER_ID[^\n]*\n(?:[^\n]*\n){0,6}?[^\n]*double ([\d.]+)d\n[^\n]*Operation\.(\w+)", static_init)
    require(sprint and sprint[2] == "ADD_MULTIPLIED_TOTAL", "sprinting speed modifier")
    sprint_amount = float(sprint[1])

    # --- Player.doSweepAttack / isSweepAttack ----------------------------------------------
    sweep_gate = body(player, "private boolean isSweepAttack(boolean, boolean, boolean);")
    require(mnemonics(sweep_gate)[:6] == ["iload_1", "ifeq", "iload_2", "ifne", "iload_3", "ifne"], "sweep gate flags")
    has_refs(sweep_gate, ["onGround", "getKnownMovement", "Vec3.horizontalDistanceSqr", "getSpeed", "Mth.square",
                          "InteractionHand.MAIN_HAND", "getItemInHand", "ItemTags.SWORDS", "ItemStack.is"], "isSweepAttack")
    require(numbers(sweep_gate) == [2.5, 0.0] and "dcmpg" in mnemonics(sweep_gate) and "ifge" in mnemonics(sweep_gate),
            "sweep speed comparison is strictly less than")
    # The call: isSweepAttack(strong, critical, sprintKnockback) with the three flags in that order.
    call = re.search(r"iload\s+7\n[^\n]*iload\s+9\n[^\n]*iload\s+8\n[^\n]*isSweepAttack", attack)
    require(call, "isSweepAttack arguments are (strong, critical, sprintKnockback)")
    crit_call = re.search(r"iload\s+7\n\s*\d+: ifeq\s+\d+\n[^\n]*aload_0\n[^\n]*aload_1\n[^\n]*canCriticalAttack", attack)
    require(crit_call, "critical requires a strong attack")
    sprint_call = re.search(r"isSprinting[^\n]*\n[^\n]*ifeq[^\n]*\n[^\n]*iload\s+7\n[^\n]*ifeq[^\n]*\n[^\n]*aload_0\n[^\n]*PLAYER_ATTACK_KNOCKBACK",
                            attack)
    require(sprint_call, "sprint knockback requires sprinting and a strong attack")
    sweep_after_hit = re.search(r"hurtOrSimulate[^\n]*\n[^\n]*istore\s+14\n[^\n]*iload\s+14\n[^\n]*ifeq[^\n]*\n(?:[^\n]*\n){0,40}?"
                                r"[^\n]*iload\s+11\n[^\n]*ifeq[^\n]*\n(?:[^\n]*\n){0,8}?[^\n]*doSweepAttack", attack)
    require(sweep_after_hit, "the sweep follows a landed primary hit")
    do_sweep = body(player, "private void doSweepAttack(net.minecraft.world.entity.Entity, float, "
                            "net.minecraft.world.damagesource.DamageSource, float);")
    has_refs(do_sweep, ["SoundEvents.PLAYER_ATTACK_SWEEP", "playServerSideSound", "level", "SWEEPING_DAMAGE_RATIO",
                        "getAttributeValue", "level", "Entity.getBoundingBox", "AABB.inflate", "Level.getEntitiesOfClass",
                        "isAlliedTo", "ArmorStand.isMarker", "distanceToSqr", "getEnchantedDamage", "LivingEntity.hurtServer",
                        "getYRot", "Mth.sin", "getYRot", "Mth.cos", "LivingEntity.knockback",
                        "EnchantmentHelper.doPostAttackEffects", "getYRot", "Mth.sin", "getYRot", "Mth.cos",
                        "ParticleTypes.SWEEP_ATTACK", "getX", "getY", "getZ", "ServerLevel.sendParticles"], "doSweepAttack")
    require(numbers(do_sweep) == [1.0, 1.0, 0.25, 1.0, 9.0, 0.4000000059604645, 0.017453292, 0.017453292, 0.017453292,
                                  0.017453292, 0.5, 0.0, 0.0, 0.0],
            f"doSweepAttack constants {numbers(do_sweep)}")
    ops = mnemonics(do_sweep)
    require(ops.count("if_acmpeq") == 2, "excludes the attacker and the primary target")
    require("instanceof" in ops, "armor stand check")
    damage_form = re.search(r"fconst_1\n[^\n]*aload_0\n[^\n]*SWEEPING_DAMAGE_RATIO[^\n]*\n[^\n]*getAttributeValue[^\n]*\n[^\n]*d2f\n"
                            r"[^\n]*fload_2\n[^\n]*fmul\n[^\n]*fadd\n[^\n]*fstore\s+6", do_sweep)
    require(damage_form, "sweep damage is 1 + ratio * damage")
    enchanted_form = re.search(r"fload\s+6\n[^\n]*aload_3\n[^\n]*getEnchantedDamage[^\n]*\n[^\n]*fload\s+4\n[^\n]*fmul", do_sweep)
    require(enchanted_form, "swept damage is the enchanted damage times the attack strength")
    require(window(do_sweep, ["ldc2_w", "aload_0", "getYRot", "ldc_w", "fmul", "f2d", "Mth.sin", "f2d", "aload_0", "getYRot", "ldc_w",
                              "fmul", "f2d", "Mth.cos", "fneg", "f2d", "aload_3", "fload", "LivingEntity.knockback"])
            and "0.4000000059604645d" in do_sweep,
            "sweep knockback pushes along the attacker's facing (sin, -cos)")
    require(window(do_sweep, ["Entity.getBoundingBox", "dconst_1", "ldc2_w", "dconst_1", "AABB.inflate"])
            and "double 0.25d" in do_sweep, "sweep box inflation")
    require(window(do_sweep, ["distanceToSqr", "ldc2_w", "dcmpg", "ifge"]) and "double 9.0d" in do_sweep,
            "sweep distance is strictly less than nine")
    require(window(do_sweep, ["aload_0", "getYRot", "ldc_w", "fmul", "f2d", "Mth.sin", "fneg", "f2d", "dstore"])
            and window(do_sweep, ["aload_0", "getYRot", "ldc_w", "fmul", "f2d", "Mth.cos", "f2d", "dstore"]),
            "sweep particle offset (-sin(yaw), cos(yaw))")
    require(window(do_sweep, ["ParticleTypes.SWEEP_ATTACK", "aload_0", "getX", "dload", "dadd", "aload_0", "ldc2_w", "getY",
                              "aload_0", "getZ", "dload", "dadd", "iconst_0", "dload", "dconst_0", "dload", "dconst_0",
                              "sendParticles"]), "sweep particle: count 0, velocity (-sin, 0, cos), speed 0")
    visual = body(player, "private void attackVisualEffects(net.minecraft.world.entity.Entity, boolean, boolean, boolean, boolean, float);")
    require(mnemonics(visual)[:4] == ["iload_2", "ifeq", "aload_0", "getstatic"], "critical sound first")
    has_refs(visual, ["SoundEvents.PLAYER_ATTACK_CRIT", "playServerSideSound", "crit", "SoundEvents.PLAYER_ATTACK_STRONG",
                      "SoundEvents.PLAYER_ATTACK_WEAK", "playServerSideSound", "magicCrit"], "attackVisualEffects")
    require(re.search(r"iload_2\n[^\n]*ifne\s+47\n[^\n]*iload_3\n[^\n]*ifne\s+47\n[^\n]*iload\s+5\n[^\n]*ifne\s+47", visual)
            or mnemonics(visual)[16:22] == ["iload_2", "ifne", "iload_3", "ifne", "iload", "ifne"],
            "strong/weak sound only without a critical or a sweep")

    # --- Independent model: Java arithmetic over the extracted constants -------------------
    K = {"resist_scale": 1.0, "halve": 2.0, "cap": 0.4, "tiny": 9.999999747378752e-06, "jitter": 0.01}
    sixth = 0.6
    default_strength = 0.4000000059604645
    sprint_bonus = 0.5
    half = 2.0

    tiny = K["tiny"]

    def unit(x, z):
        """Vec3.normalize of (x, 0, z): zero below a length of 1.0E-5f."""
        length = math.sqrt(x * x + z * z)
        return (0.0, 0.0) if length < tiny else (x / length, z / length)

    def horizontal(old, strength, resistance, x, z, on_ground):
        strength = strength * (1.0 - resistance)
        if strength <= 0.0:
            return None
        ux, uz = unit(x, z)
        vx, vz = ux * strength, uz * strength
        new_y = min(K["cap"], old[1] / half + strength) if on_ground else old[1]
        return [old[0] / half - vx, new_y, old[2] / half - vz]

    def attack_strength(attribute, enchant_level, sprint):
        base = f32(f32(attribute) + enchant_level) / half
        return f32(f32(base) + (sprint_bonus if sprint else 0.0))

    knock_cases = []
    for old, strength, resistance, ground, x, z in [
        ([0.0, 0.0, 0.0], default_strength, 0.0, True, 1.0, 0.0),
        ([0.0, 0.0, 0.0], default_strength, 0.0, False, 1.0, 0.0),
        ([0.1, -0.3, 0.2], default_strength, 0.0, True, 0.6, -0.8),
        ([0.1, -0.3, 0.2], default_strength, 0.0, False, 0.6, -0.8),
        ([0.0, 0.3, 0.0], default_strength, 0.0, True, 0.0, 2.0),
        ([0.0, 0.0, 0.0], 0.5, 0.0, True, -3.0, 4.0),
        ([0.2, 0.0, -0.1], 0.9, 0.0, True, 1.0, 1.0),
        ([0.0, 0.0, 0.0], default_strength, 0.5, True, 1.0, 0.0),
        ([0.0, 0.0, 0.0], default_strength, 0.6000000238418579, True, 1.0, 0.0),
        ([0.0, 0.0, 0.0], default_strength, 1.0, True, 1.0, 0.0),
        ([0.0, 0.0, 0.0], default_strength, -1.0, True, 1.0, 0.0),
        ([0.0, 0.0, 0.0], default_strength, 0.1, True, 0.0, -1.0),
        ([0.0, 0.0, 0.0], default_strength, 0.4, False, 1.0, 0.0),
        ([0.0, 0.1, 0.0], 0.0, 0.0, True, 1.0, 0.0),
    ]:
        new = horizontal(old, strength, resistance, x, z, ground)
        knock_cases.append({"old_motion": old, "strength": strength, "resistance": resistance, "on_ground": ground,
                            "x": x, "z": z, "new_motion": new})

    attack_cases = []
    for attribute in (0.0, 1.0, 1.5):
        for level in (0, 1, 2):
            for sprint in (False, True):
                attack_cases.append({"attack_knockback_attribute": attribute, "knockback_level": level,
                                     "sprint_bonus": sprint, "strength": attack_strength(attribute, level, sprint)})

    yaw_cases = []
    for degrees in (0.0, 45.0, 90.0, 135.0, 180.0, -90.0, 270.0, 33.3, -170.25, 361.0):
        radians = f32(f32(degrees) * f32(0.017453292))
        yaw_cases.append({"yaw_degrees": degrees, "sin": sin(float(radians)), "cos": cos(float(radians)),
                          "push_x": float(sin(float(radians))), "push_z": -float(cos(float(radians)))})

    arrow_cases = []
    for level in (0, 1, 2, 3):
        for resistance in (0.0, 0.5, 0.6000000238418579, 1.0, -1.0):
            for velocity in ([1.0, -0.5, 0.0], [0.0, 0.3, 2.0], [-3.0, 0.0, 4.0], [0.0, 1.0, 0.0],
                             [1e-6, -1.0, 0.0], [0.0, -1.0, -9e-6], [1.1e-5, 0.0, 0.0]):
                strength = float(f32(float(level)))
                scale = max(0.0, 1.0 - resistance)
                ux, uz = unit(velocity[0], velocity[2])
                scaled = strength * sixth * scale
                vector = [ux * scaled, 0.0, uz * scaled]
                if strength <= 0.0:
                    push = None
                else:
                    push = [vector[0], 0.1, vector[2]] if sum(c * c for c in vector) > 0.0 else None
                arrow_cases.append({"punch_level": level, "resistance": resistance, "arrow_motion": velocity, "push": push})

    def throw_push(strength, resistance, dx, dz, turn, reach, lift):
        remaining = strength - resistance
        if remaining <= 0.0:
            return None
        scale = float(f32(f32(reach * 0.5) + f32(0.2))) * remaining
        ux, uz = unit(dx, dz)
        ax, az = ux * scale, uz * scale
        c, s = float(cos(turn)), float(sin(turn))
        return [ax * c + az * s, remaining * float(lift) * 0.5, az * c - ax * s]

    throw_cases = []
    for resistance in (0.0, 0.5, 1.0, 1.2):
        for dx, dz in [(1.0, 0.0), (0.0, -2.0), (0.6, 0.8), (1e-6, 0.0)]:
            for turn in (-10.0, -3.0, 0.0, 4.0, 10.0):
                for reach, lift in [(0.0, 0.0), (0.5, 0.25), (0.9999, 0.75)]:
                    throw_cases.append({"attack_knockback": 1.0, "resistance": resistance, "dx": dx, "dz": dz, "turn": turn,
                                        "reach_roll": reach, "lift_roll": lift,
                                        "push": throw_push(1.0, resistance, dx, dz, turn, f32(reach), f32(lift))})

    def sweep_allowed(strong, critical, sprint_knockback, on_ground, horizontal_sqr, speed, sword):
        limit = f32(speed) * 2.5
        return bool(strong and not critical and not sprint_knockback and on_ground
                    and horizontal_sqr < limit * limit and sword)

    walking = f32(player_speed)
    speed_values = {"walking": walking}
    for level in (1, 2, 3):
        speed_values[f"speed_{level}"] = f32(walking * (1.0 + speed_effects["speed"]["amount_per_level"] * level))
        speed_values[f"slowness_{level}"] = f32(walking * (1.0 + speed_effects["slowness"]["amount_per_level"] * level))
    eligibility = []
    for strong, critical, sprint_knockback, ground, sword in [
        (True, False, False, True, True), (False, False, False, True, True), (True, True, False, True, True),
        (True, False, True, True, True), (True, False, False, False, True), (True, False, False, True, False),
    ]:
        eligibility.append({"strong": strong, "critical": critical, "sprint_knockback": sprint_knockback,
                            "on_ground": ground, "sword": sword, "horizontal_distance_sqr": 0.0, "speed": walking,
                            "allowed": sweep_allowed(strong, critical, sprint_knockback, ground, 0.0, walking, sword)})
    for name, speed_value in speed_values.items():
        limit = (f32(speed_value) * 2.5) * (f32(speed_value) * 2.5)
        # The exact boundary (`<`, so equal is refused) is checked in the test; decimal JSON may not round-trip it.
        for offset in (-1e-6, 1e-6):
            sqr = limit + offset
            eligibility.append({"strong": True, "critical": False, "sprint_knockback": False, "on_ground": True,
                                "sword": True, "horizontal_distance_sqr": sqr, "speed": speed_value, "speed_name": name,
                                "allowed": sweep_allowed(True, False, False, True, sqr, speed_value, True)})

    sweep_damage = []
    for damage in (0.0, 1.0, 2.5, 4.0, 7.0, 12.0):
        for ratio_level in (0, 1, 2, 3):
            ratio = sweeping_ratio.get(str(ratio_level), 0.0)
            swept = f32(1.0 + f32(f32(ratio) * f32(damage)))
            for charge in (0.0, 0.3, 0.9, 1.0):
                for enchant_bonus in (0.0, 1.5):
                    sweep_damage.append({"scaled_attack_damage": damage, "sweeping_edge_level": ratio_level,
                                         "attack_strength": charge, "victim_enchantment_bonus": enchant_bonus,
                                         "damage": f32(f32(swept + enchant_bonus) * f32(charge))})

    # Target box inflation, victim candidates and distance.
    def inflate(box, dx, dy, dz):
        return [box[0] - dx, box[1] - dy, box[2] - dz, box[3] + dx, box[4] + dy, box[5] + dz]

    def intersects(a, b):
        return a[0] < b[3] and a[3] > b[0] and a[1] < b[4] and a[4] > b[1] and a[2] < b[5] and a[5] > b[2]

    def mob_box(x, y, z, half_width, height):
        return [x - half_width, y, z - half_width, x + half_width, y + height, z + half_width]

    target = mob_box(0.0, 0.0, 0.0, 0.3, 1.95)
    swept_box = inflate(target, 1.0, 0.25, 1.0)
    reach_cases = []
    for victim_xyz, half_width, height, attacker in [
        ((1.3, 0.0, 0.0), 0.3, 1.95, (-1.0, 0.0, 0.0)),
        ((1.6, 0.0, 0.0), 0.3, 1.95, (-1.0, 0.0, 0.0)),
        ((1.5999, 0.0, 0.0), 0.3, 1.95, (-1.0, 0.0, 0.0)),
        ((0.0, 2.2, 0.0), 0.3, 1.95, (0.0, 0.0, -1.0)),
        ((0.0, 2.1, 0.0), 0.3, 1.95, (0.0, 0.0, -1.0)),
        ((0.0, -2.2, 0.0), 0.3, 1.95, (0.0, 0.0, -1.0)),
        ((0.0, -2.1, 0.0), 0.3, 1.95, (0.0, 0.0, -1.0)),
        ((1.0, 0.0, 1.0), 0.45, 0.9, (-1.0, 0.0, -1.0)),
        ((2.6, 0.0, 0.0), 0.3, 1.95, (-0.4, 0.0, 0.0)),
        ((1.0, 0.0, 0.0), 0.3, 1.95, (-2.0, 0.0, 0.0)),
        ((1.0, 0.0, 0.0), 0.3, 1.95, (-1.9999, 0.0, 0.0)),
        ((1.0, 0.0, 0.0), 0.3, 1.95, (-2.0001, 0.0, 0.0)),
    ]:
        box = mob_box(*victim_xyz, half_width, height)
        reach_cases.append({"victim_position": list(victim_xyz), "victim_half_width": half_width, "victim_height": height,
                            "attacker_position": list(attacker),
                            "in_box": intersects(swept_box, box),
                            "distance_sqr": sum((a - b) ** 2 for a, b in zip(attacker, victim_xyz)),
                            "swept": intersects(swept_box, box) and sum((a - b) ** 2 for a, b in zip(attacker, victim_xyz)) < 9.0})

    sounds = []
    for strong in (False, True):
        for sprinting in (False, True):
            for critical in (False, True):
                for sweep in (False, True):
                    for landed in (False, True):
                        events = []
                        if sprinting and strong:
                            events.append("player.attack.knockback")
                        if landed:
                            if critical:
                                events.append("player.attack.crit")
                            if sweep:
                                events.append("player.attack.sweep")
                            if not critical and not sweep:
                                events.append("player.attack.strong" if strong else "player.attack.weak")
                        else:
                            events.append("player.attack.nodamage")
                        sounds.append({"strong": strong, "sprinting": sprinting, "critical": critical, "sweep": sweep,
                                       "primary_hit_landed": landed, "events": events})

    return {
        "mth_sin": {"table_size": 65536, "scale": 10430.378350470453, "cos_offset": 16384.0, "mask": 65535,
                    "entry": "float(sin(index / scale))"},
        "attributes": {**attributes, "movement_speed_player_base": player_speed,
                       "movement_speed_range": [0.0, float(movement[3])]},
        "living_entity_knockback": {
            "resistance_attribute": "knockback_resistance",
            "strength_scale": "strength * (1 - knockback_resistance); nothing happens when the result is <= 0",
            "zero_direction_threshold": K["tiny"], "zero_direction_jitter": K["jitter"],
            "horizontal_motion_divisor": half, "vertical_cap": K["cap"],
            "vertical": "on ground: min(vertical_cap, motion_y / 2 + strength); airborne: motion_y unchanged",
            "default_strength": default_strength, "default_strength_float": float(default_constant[1]),
            "hurt_direction": {
                "direction_is": "vector from the victim toward the damage source, so the victim is pushed away",
                "projectile": "the projectile's horizontal delta movement, negated",
                "other": "damage source position (the direct entity's position) minus the victim's position",
            },
            "knockback_only_when": "hurtServer applied damage, DamageTypeTags.NO_KNOCKBACK is absent, and the hit was not fully blocked",
            "damage_cooldown_ticks": 10.0,
            "cooldown_rejects_hits_not_exceeding_last_damage": True,
        },
        "attack_knockback": {
            "attribute": "attack_knockback", "enchantment": "knockback",
            "enchantment_per_level": 1.0, "divisor": half,
            "player_sprint_bonus": sprint_bonus,
            "total": "(attack_knockback + enchantment_level) / 2 (+ 0.5 when sprint knockback)",
            "direction": "attacker yaw: x = sin(yaw), z = -cos(yaw); the victim is pushed along its facing",
            "applied_after": "the default 0.4 hurt knockback",
            "attacker_after": {"horizontal_motion_multiplier": sixth, "vertical_multiplier": 1.0, "player_sprinting": False,
                               "only_when_strength_positive": True},
            "non_living_target": {"push": "(-sin(yaw) * strength, 0.1, cos(yaw) * strength)"},
        },
        "mob_melee": {
            "default_hurt_knockback": default_strength,
            "extra": "getKnockback = attack_knockback attribute / 2 (weapon enchantment applies), along the attacker's yaw",
            "attacker_after": {"horizontal_motion_multiplier": sixth},
            "attributes": mob_attributes,
        },
        "hoglin_throw": {
            "formula": "strength = attack_knockback - resistance (nothing when <= 0); horizontal = normalize(dx, 0, dz) * "
                       "strength * (nextFloat * 0.5 + 0.2) turned by yRot(nextInt(21) - 10) in radians; vertical = strength * "
                       "nextFloat * 0.5; added to the victim's motion after the default hurt knockback",
            "turn_unit": "radians", "turn_range": [-10, 10], "reach_range": [0.2, 0.7], "lift_max": 0.5,
            "no_extra_knockback": True, "not_when_baby": True,
        },
        "arrow_knockback": {
            "enchantment": "punch", "enchantment_applies_to": "arrow direct attackers (#minecraft:arrows)",
            "arrow_entity_type_tag": sorted(arrows),
            "per_level_push": sixth, "vertical_push": 0.1,
            "horizontal_direction": "the arrow's horizontal delta movement, normalized",
            "resistance": "max(0, 1 - knockback_resistance)",
            "default_knockback": "the 0.4 hurt knockback along the arrow's horizontal motion",
        },
        "movement_speed": {
            "player_base": player_speed, "sprint_modifier": sprint_amount, "sprint_operation": "add_multiplied_total",
            "effects": speed_effects, "range": [0.0, float(movement[3])],
        },
        "sweep_attack": {
            "requires": {"attack_strength_above": 0.9, "not_critical": True, "not_sprint_knockback": True,
                         "on_ground": True, "horizontal_movement_below_speed_times": 2.5,
                         "movement_is": "Player.getKnownMovement().horizontalDistanceSqr() < (getSpeed() * 2.5)^2",
                         "main_hand": "#minecraft:swords"},
            "sword_items": sorted(swords),
            "after_primary_hit_landed": True,
            "target_box": {"inflate": [1.0, 0.25, 1.0], "of": "the primary target's bounding box"},
            "victims": "LivingEntity intersecting the box",
            "max_distance_sqr": 9.0, "distance_between": "attacker position and victim position (feet)",
            "excluded": ["the attacker", "the primary target", "allied entities (scoreboard team)", "marker armor stands"],
            "damage": {"base": 1.0, "ratio_attribute": "sweeping_damage_ratio", "ratio_times": "scaled attack damage",
                       "then": "enchantment damage for that victim", "times": "attack strength scale"},
            "knockback": {"strength": default_strength, "direction": "attacker yaw: x = sin(yaw), z = -cos(yaw)",
                          "after": "the victim's own 0.4 hurt knockback from the attacker's position"},
            "sound": "player.attack.sweep", "particle": "sweep_attack",
            "particle_position": "(attacker x - sin(yaw), attacker y + 0.5 * height, attacker z + cos(yaw))",
            "particle_count": 0,
            "sweeping_edge_ratio": sweeping_ratio,
            "sweeping_edge_max_level": sweeping_enchant["max_level"],
        },
        "cases": {
            "knockback": knock_cases,
            "attack_knockback": attack_cases,
            "yaw_direction": yaw_cases,
            "arrow_punch": arrow_cases,
            "hoglin_throw": throw_cases,
            "sweep_eligibility": eligibility,
            "sweep_damage": sweep_damage,
            "sweep_reach": reach_cases,
            "attack_sounds": sounds,
            "player_speed": {k: v for k, v in speed_values.items()},
        },
    }
