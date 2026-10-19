# Sweeping and knockback — 2026-10-17

[Roadmap](../roadmap.md) · [Shared combat](2026-10-13-shared-combat.md) · [Reference fixtures](../../tests/fixtures/java/26.3/README.md)

Scope: PLY-05a, based on `77a6885` on `agent/combat-fixtures`. Reference: official
Java Edition 26.3 server SHA-1 `33680f5f2ac32864d6d7cf5e56a705fdb3e05f4c`.
Checks ran headless on the M5 MacBook Pro; no graphical run or benchmark.

## Java classes read

`Player` (`attack`, `isSweepAttack`, `doSweepAttack`, `attackVisualEffects`,
`canCriticalAttack`, `causeExtraKnockback`, `getSpeed`, `createAttributes`),
`LivingEntity` (`knockback` with and without the unused flag, `getKnockback`,
`dealDefaultKnockback`, `hurtServer`, `causeExtraKnockback`, `DEFAULT_KNOCKBACK`,
the sprinting modifier), `Mob.doHurtTarget`, `Entity.push`, `ServerPlayer`/`Player`
`getEnchantedDamage`, `AbstractArrow` (`onHitEntity`, `doKnockback`),
`Projectile.calculateHorizontalHurtKnockbackDirection`,
`DamageSource.getSourcePosition`, `HoglinBase`, `Hoglin`, `Zoglin`, `Vec3.yRot`,
`Mth` (the sine table, `sin`, `cos`), `Attributes`, `MobEffects`, the
`createAttributes` of every entity that sets `knockback_resistance` or
`attack_knockback` (iron golem, hoglin, zoglin, ravager, warden, nautilus) and the
data `enchantment/{knockback,punch,sweeping_edge}`, `tags/item/swords` and
`tags/entity_type/arrows`.

## Java rules extracted (`combat.json`)

- **Hurt knockback.** `hurtServer` calls `dealDefaultKnockback` (strength
  `0.4f`) whenever damage applies and the type is not `NO_KNOCKBACK`; the vector
  points from the victim toward `DamageSource.getSourcePosition()` (the attacker's
  feet), so the victim moves *away from the attacker's position*, not along the
  view. A projectile uses its negated horizontal motion instead.
- **`LivingEntity.knockback(strength, x, z)`.** `strength *= 1 - knockback_resistance`
  (nothing at or below zero); a direction shorter than `sqrt(1.0E-5)` is replaced
  by `(nextDouble - nextDouble) * 0.01` pairs; `motion.x/z = old / 2 - normalize(x, z) *
  strength`; vertical is `min(0.4, old.y / 2 + strength)` on the ground and
  unchanged in the air. The six-argument flag is not read.
- **Player attack knockback.** `getKnockback = (attack_knockback + Knockback level) / 2`
  (the player's attribute is 0), plus `0.5` for a sprinting strong attack, applied
  after the hurt push along the attacker's yaw `(sin, -cos)` using `Mth`'s sine
  table. When positive it also multiplies the attacker's horizontal motion by 0.6
  and stops its sprint. Non-living targets get `push(-sin·s, 0.1, cos·s)` instead.
- **Mob melee.** `Mob.doHurtTarget` is the same: hurt push, then `getKnockback`
  (attribute / 2) along the mob's yaw. Hoglins and zoglins instead use
  `HoglinBase.hurtAndThrowTarget`: hurt push, then `throwTarget`, an additive push of
  `attack_knockback (1.0) - resistance` scaled by `nextFloat * 0.5 + 0.2`,
  turned by `yRot(nextInt(21) - 10)` **in radians** (unconverted, so effectively
  a random direction) with a lift of `strength * nextFloat * 0.5`; no
  `causeExtraKnockback`.
- **Arrows.** Hurt push along the arrow's horizontal motion, then Punch adds
  `normalize(horizontal motion) * level * 0.6 * max(0, 1 - resistance)` with a 0.1
  lift through `push` (additive).
- **Knockback resistance.** Attribute range -2..1. Iron golem 1; hoglin and zoglin
  `0.6f`; ravager 0.75 and warden 1 (not implemented); armor adds 0.1 per Netherite piece.
- **Sweep.** After a landed primary hit, `isSweepAttack(strong, critical, sprintKnockback)`:
  strong (> 0.9), not critical, not a sprint-knockback attack, on the ground, last-tick
  horizontal movement squared `< (getSpeed() * 2.5)²` (`getSpeed` is the movement-speed
  attribute: 0.1 base, Speed ×(1 + 0.2·level), Slowness ×(1 − 0.15·level),
  `add_multiplied_total`), and a main-hand item in `#swords` (all seven incl. copper).
  `doSweepAttack` plays `player.attack.sweep`, then for each `LivingEntity` in the primary
  target's box inflated by (1, 0.25, 1), excluding the attacker, the primary target, allies
  (scoreboard teams) and marker armor stands, within `distanceToSqr < 9`: damage
  `enchantedDamage(1 + sweeping_damage_ratio · scaledDamage) · attackStrength`
  (ratio `level / (level + 1)`), then the victim's own hurt push and a 0.4 knockback along
  the attacker's yaw, plus post-attack enchantment effects. The particle is `sweep_attack`
  at `(x − sin yaw, y + 0.5·height, z + cos yaw)`. Sounds: knockback (sprinting strong, before
  the hit), then on a hurt target crit and/or sweep, and strong/weak only when neither; no
  damage plays `player.attack.nodamage`.

## Differences found and fixed

| Area | Before | Now |
| --- | --- | --- |
| Hurt direction | the attacker's 3-D view ray, flattened | attacker→victim feet vector (`Push::hurt`), then the facing push |
| Knockback strength | fixed 6.0 b/s ×(1 + 1.25·extra), +5 b/s up | 0.4 per push ×20 b/s, extra = (level)/2 (+0.5 sprint), two pushes |
| Existing motion | replaced | halved by each push; vertical `min(0.4, y/2 + s)` on the ground only |
| Airborne targets | popped upward anyway | vertical unchanged |
| Resistance | an iron golem special case; hoglin share 0.4 | attribute-driven for every push (`knockback_resistance`) |
| Sprint/enchant slowdown | none | attacker's horizontal motion ×0.6 and sprint stops when extra knockback > 0 |
| Sweep speed limit | fixed 0.25 b/tick | `getSpeed() · 2.5` with Speed/Slowness |
| Speed + Slowness | added | multiplied (`add_multiplied_total`) |
| Sweep box/damage/push | box and damage right; push along the view ray | hurt push then facing push of 0.4 |
| LAN PvP | 0.4–1.0 b/s shove, ~no motion | same shared rules and push chain as mobs |
| Mob melee on players | `dir·6 + 5` b/s added | Java push chain through `PlayerSession::knocked_back` |
| Arrow Punch | `4·(1+3·level)` b/s | hurt push + 0.6/level shove with resistance |
| Hoglin toss | ±10° | ±10 rad, `attack_knockback − resistance`, applied by the victim |
| Sweep side effects | swept mobs and the particle had no attacker identity or facing | swept zombified piglins anger like a direct hit, a swept iron golem targets the attacker, and the poof appears at `(x − sin yaw, y + 0.5·height, z + cos yaw)` |
| Sprint cancel | no slowdown, no sprint stop | `Outcome::Hit::sprint_cancelled` slows motion ×0.6; the CLI agent clears its sprint |

## Evidence

`tools/extract_java_knockback_facts.py` (called by `extract_java_combat_fixtures.py`)
adds these facts and reference cases to `combat.json`; every method is checked against
its exact call and constant sequence, so a changed bytecode form fails the extraction.
Regenerating from the pinned artifact reproduces the committed fixture byte for byte, and
all previous sections are unchanged. `tests/java_knockback.rs` (37 tests) checks the pure
rules in `src/knockback.rs` against 14 knockback, 18 attack-strength, 10 yaw, 140 Punch,
240 hoglin-toss, 20 sweep-eligibility, 192 sweep-damage, 12 sweep-reach and 32 sound
cases, then runs whole-game scenarios against an evaluator built from the fixture's
constants (itself pinned to the fixture cases):

- hurt direction from positions, not the view; sprint and Knockback I/II second push along the
  facing, with and without sprint, with a weak (low charge) hit, with existing motion halved
  by each push, an airborne target, and knockback-resistant iron golems (1.0), hoglins and
  zoglins (0.6);
- attacker slowdown (×0.6) and sprint cancel only when the extra knockback is positive;
- sweep victims (target, in-box, outside the box, beyond three blocks, a quarter block
  above), damage with Sweeping Edge 0/1/3 and a Smite victim bonus scaled by charge, the
  victims' two-push knockback, every gate (weak, sprinting, airborne, critical, movement at
  and over 2.5× speed with Speed I and Slowness II) and every sword in `#swords` (including
  copper) against non-swords and bare hands, plus the strict `<` boundary;
- the sweep's side effects: a swept zombified piglin angers its neighbours like any player
  hit, a swept iron golem turns on the attacker (not the host), the sweep particle appears
  one block ahead of the attacker at `(x − sin yaw, y + 0.5·height, z + cos yaw)` from the
  fixture's sine table, a critical (falling) hit never sweeps, and a CLI agent's sprint
  stops after a hit with extra knockback;
- `Outcome::sounds()` against the sound table, the CLI agent producing the same knockback
  as the session, mob melee and hoglin events carrying Java pushes, a Netherite-armored
  player's resistance, and arrow Punch on mobs and players (plus an arrow flight through the
  entity update).

`cargo fmt && cargo clippy --release --all-targets -- -D warnings && cargo test --release &&
cargo clippy --release --no-default-features --all-targets -- -D warnings &&
cargo test --release --no-default-features` all passed: release **1229 passed, zero failed,
nine ignored**; device-independent **1141 passed, zero failed, eight ignored**.
Not run: the real desktop LAN harness and any graphical or benchmark check.

## Integration review fixes

A Codex review of the merge into `t3/roadmap-next-stage-v0-8` found five
differences, fixed with tests in `tests/java_knockback.rs`:

- A stronger hit inside a player's hurt-immunity window deals the difference
  without `hurtServer`'s default push or hurt direction; the attacker's extra
  knockback, Punch and a hoglin toss still apply (`Vitals::last_hit_fresh`,
  `Hit::without_default`). The desktop and CLI-agent damage paths keep the
  current hurt direction (camera tilt) on such a hit, which a second review
  round caught.
- LAN PvP: a hit the target refuses (Creative, hurt immunity) no longer slows the
  attacker, stops its sprint, sweeps, sets fire or triggers Thorns.
- Angry wolves bite through `Mob.doHurtTarget`'s push (`Hit::melee`) instead of
  the old impulse.
- Swept mobs retaliate and record villager gossip like the main target (a swept
  wild wolf turns on the attacker).
- Full knockback resistance leaves an iron golem's motion untouched on the
  legacy (mob-on-mob) impulse path.

## Remaining differences

- Mob locomotion after the impulse is VoxelCraft's (gravity 28 and exponential
  horizontal easing), so travel distance and flight time differ even though the
  impulse equals Java's ×20.
- Java's 10-tick damage cooldown rejects a hit not exceeding the last damage and gives
  it no knockback; VoxelCraft mobs have no such window.
- Sweeps start only from mob targets: not from hitting a minecart, boat, crystal or
  dragon part; swept victims are mobs (other players only in LAN PvP, and a mob-target
  sweep does not hurt nearby players); scoreboard teams do not exist; no armor stands.
- Non-living targets are not pushed by the extra knockback (their motion is unmodeled).
- The sweep particle is the generic poof (no `sweep_attack` sprite); the knockback, strong,
  weak, sweep and no-damage sounds are exposed in `Outcome::sounds()` but the game still
  plays its single hit sound.
- Other sources keep their own impulses: explosions, golem flings, dragon, ghast
  fireballs, llama spit, thrown items and mob-versus-mob hits.
- Zombie reinforcement bonuses to knockback resistance, the pre-hit-motion restore for
  `ServerPlayer` targets (`syncVelocity`) and ravager/warden are not modeled.
- LAN PvP shares the pure rules and `Hit::melee`, but `src/app/lan.rs` is part of the
  desktop binary and has no library test; the real-desktop harness was not run.

State: **Implemented, unverified** for PLY-05a. Ordinary keyboard/controller play,
feel, audio and visual feedback remain unverified.
