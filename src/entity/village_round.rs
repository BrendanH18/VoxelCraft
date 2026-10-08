//! Village life beyond residents: golem patterns, summons, and who they hit.
//!
//! Iron-golem summoning is a documented simplification of Java's spawn
//! behavior: a panicking adult with 3 villagers and 3 beds within 16 blocks,
//! and no iron golem already there, summons one. Once a minute a cluster of
//! 5 villagers and 5 beds has a 10% chance to summon without a panic. A
//! summon then waits 30 seconds.
//!
//! Player-built golems use Java's patterns (the pumpkin is placed last).
//! Iron: a T of iron blocks under the pumpkin, arms on either horizontal
//! axis. Snow: two snow blocks under the pumpkin. Jack o'lanterns count.
use glam::{DVec3, IVec3};

use super::thrown::{self, Thrown};
use super::villager::{self, Profession};
use super::{Ctx, Entities, EntityEvent, MobKind, MobWorld, PlayerId};
use crate::simulation::difficulty::Difficulty;
use crate::world::block::{Block, Facing};
use crate::world::pumpkin_blocks;

/// Cells a freshly placed head turned into a golem, pumpkin included.
pub enum Pattern {
    Iron { yaw: f32, cells: [IVec3; 5] },
    Snow { yaw: f32, cells: [IVec3; 3] },
}

fn facing_yaw(facing: Facing) -> f32 {
    match facing {
        Facing::East => 0.0,
        Facing::South => std::f32::consts::FRAC_PI_2,
        Facing::West => std::f32::consts::PI,
        Facing::North => -std::f32::consts::FRAC_PI_2,
    }
}

/// Java's build, if `pos` is a carved pumpkin or jack o'lantern placed last.
pub fn pattern_at(world: &impl crate::physics::BlockSource, pos: IVec3) -> Option<Pattern> {
    let head = world.block(pos)?;
    if !pumpkin_blocks::is_head(head) {
        return None;
    }
    let yaw = facing_yaw(head.oriented().map_or(Facing::South, |(_, f)| f));
    let neck = pos - IVec3::Y;
    let hip = neck - IVec3::Y;
    if world.block(neck) == Some(Block::IRON_BLOCK) && world.block(hip) == Some(Block::IRON_BLOCK) {
        for (a, b) in [(IVec3::X, IVec3::NEG_X), (IVec3::Z, IVec3::NEG_Z)] {
            if world.block(neck + a) == Some(Block::IRON_BLOCK) && world.block(neck + b) == Some(Block::IRON_BLOCK) {
                return Some(Pattern::Iron { yaw, cells: [pos, neck, hip, neck + a, neck + b] });
            }
        }
    }
    if world.block(neck) == Some(Block::SNOW) && world.block(hip) == Some(Block::SNOW) {
        return Some(Pattern::Snow { yaw, cells: [pos, neck, hip] });
    }
    None
}

/// Kind, yaw and feet position for a finished pattern.
pub fn spawn_feet(pattern: &Pattern) -> (MobKind, f32, DVec3) {
    match *pattern {
        Pattern::Iron { yaw, cells } => (MobKind::IronGolem, yaw, cells[2].as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
        Pattern::Snow { yaw, cells } => (MobKind::SnowGolem, yaw, cells[2].as_dvec3() + DVec3::new(0.5, 0.0, 0.5)),
    }
}

/// Java's listed attack strength: 7.5 / 15 / 22.5. Peaceful players take none.
pub fn iron_damage(difficulty: Difficulty, player: bool) -> f32 {
    if player && difficulty == Difficulty::Peaceful {
        return 0.0;
    }
    match difficulty {
        Difficulty::Easy | Difficulty::Peaceful => 7.5,
        Difficulty::Normal => 15.0,
        Difficulty::Hard => 22.5,
    }
}

const GOLEM_RANGE: f64 = 16.0;
const DEFEND_RANGE: f64 = 32.0;

/// Remove a finished pumpkin pattern and spawn the golem that was built.
pub fn finish_golems(world: &mut crate::world::World, entities: &mut Entities) {
    for pos in std::mem::take(&mut world.golem_heads) {
        let Some(pattern) = pattern_at(world, pos) else { continue };
        let (kind, yaw, feet) = spawn_feet(&pattern);
        let (cells, n) = match pattern {
            Pattern::Iron { cells, .. } => (cells, 5),
            Pattern::Snow { cells, .. } => ([cells[0], cells[1], cells[2], cells[0], cells[0]], 3),
        };
        for cell in cells.into_iter().take(n) {
            world.set_block(cell, Block::AIR);
        }
        entities.spawn(kind, feet);
        if let Some(mob) = entities.mobs.last_mut() {
            mob.yaw = yaw;
            mob.built = true;
        }
    }
}

impl Entities {
    /// Point iron golems at monsters near villagers, or at a player who hit them.
    /// Creepers step away from a nearby golem.
    pub(super) fn assign_hunts(&mut self, ctx: &Ctx) {
        let n = self.mobs.len();
        for i in 0..n {
            self.mobs[i].hunt = None;
            self.mobs[i].strike = false;
            if !self.mobs[i].alive() {
                continue;
            }
            let pos = self.mobs[i].pos;
            match self.mobs[i].kind {
                MobKind::IronGolem if self.mobs[i].angry_at_player() => {
                    if let Some(player) = ctx.nearest_target(pos)
                        && player.pos.distance_squared(pos) < GOLEM_RANGE * GOLEM_RANGE
                    {
                        self.mobs[i].hunt = Some(player.pos);
                    }
                }
                MobKind::IronGolem => {
                    let village = (0..n).any(|j| {
                        let o = &self.mobs[j];
                        o.alive()
                            && o.kind == MobKind::Villager
                            && o.pos.distance_squared(pos) < DEFEND_RANGE * DEFEND_RANGE
                    });
                    if !village {
                        continue;
                    }
                    let mut best: Option<(f64, DVec3)> = None;
                    for o in &self.mobs {
                        if o.alive() && o.kind.is_hostile() && o.kind != MobKind::Creeper {
                            let d = o.pos.distance_squared(pos);
                            if d < GOLEM_RANGE * GOLEM_RANGE && best.is_none_or(|(bd, _)| d < bd) {
                                best = Some((d, o.pos));
                            }
                        }
                    }
                    self.mobs[i].hunt = best.map(|(_, p)| p);
                }
                MobKind::Zombie | MobKind::Husk | MobKind::Drowned | MobKind::ZombieVillager => {
                    let mut best: Option<(f64, DVec3)> = None;
                    for o in &self.mobs {
                        if o.alive() && o.kind == MobKind::Villager {
                            let d = o.pos.distance_squared(pos);
                            if d < GOLEM_RANGE * GOLEM_RANGE && best.is_none_or(|(bd, _)| d < bd) {
                                best = Some((d, o.pos));
                            }
                        }
                    }
                    if let Some((d, at)) = best {
                        let player_closer = ctx.nearest_target(pos).is_some_and(|t| {
                            let pd = t.pos.distance_squared(pos);
                            pd < d && pd < 24.0 * 24.0
                        });
                        if !player_closer {
                            self.mobs[i].hunt = Some(at);
                        }
                    }
                }
                MobKind::Creeper => {
                    let mut away: Option<(f64, DVec3)> = None;
                    for o in &self.mobs {
                        if o.alive() && o.kind == MobKind::IronGolem {
                            let d = o.pos.distance_squared(pos);
                            if d < 36.0 && d > 1e-4 && away.is_none_or(|(bd, _)| d < bd) {
                                let flat = (pos - o.pos) * DVec3::new(1.0, 0.0, 1.0);
                                away = Some((d, pos + flat.normalize_or_zero() * 6.0));
                            }
                        }
                    }
                    self.mobs[i].hunt = away.map(|(_, p)| p);
                }
                _ => {}
            }
        }
    }

    /// Land the swings `think` asked for. Angry golems hit players; the rest hit monsters.
    pub(super) fn resolve_strikes(&mut self, ctx: &Ctx, events: &mut Vec<EntityEvent>) {
        let n = self.mobs.len();
        for i in 0..n {
            if !self.mobs[i].strike || !self.mobs[i].alive() {
                continue;
            }
            self.mobs[i].strike = false;
            let pos = self.mobs[i].pos;
            let kind = self.mobs[i].kind;
            let difficulty = self.mobs[i].difficulty;
            if kind == MobKind::IronGolem
                && self.mobs[i].angry_at_player()
                && let Some(player) = ctx.nearest_target(pos)
                && player.pos.distance_squared(pos) < 9.0
            {
                let dir = (player.pos - pos).normalize_or_zero();
                events.push(EntityEvent::PlayerHit {
                    player: player.id,
                    damage: iron_damage(difficulty, true),
                    knockback: (dir * 6.0 + DVec3::Y * 8.0).as_vec3(),
                    cause: "was slain by an iron golem",
                });
                continue;
            }
            if kind.is_zombie() {
                let mut bitten: Option<usize> = None;
                let mut nearest = 2.25f64;
                for j in 0..n {
                    if i != j && self.mobs[j].alive() && self.mobs[j].kind == MobKind::Villager {
                        let d = self.mobs[j].pos.distance_squared(pos);
                        if d < nearest {
                            nearest = d;
                            bitten = Some(j);
                        }
                    }
                }
                if let Some(j) = bitten {
                    self.infect_or_hit(i, j, events);
                }
                continue;
            }
            let mut victim: Option<usize> = None;
            let mut best = f64::MAX;
            for j in 0..n {
                if i == j || !self.mobs[j].alive() || kind != MobKind::IronGolem {
                    continue;
                }
                if self.mobs[j].kind.is_hostile() && self.mobs[j].kind != MobKind::Creeper {
                    let d = self.mobs[j].pos.distance_squared(pos);
                    if d < 9.0 && d < best {
                        best = d;
                        victim = Some(j);
                    }
                }
            }
            let Some(j) = victim else { continue };
            self.hit_mob(i, j, iron_damage(difficulty, false), 8.0, events);
        }
    }

    /// A killing blow from a zombie converts the villager on Normal (50%) and Hard (100%).
    fn infect_or_hit(&mut self, attacker: usize, victim: usize, events: &mut Vec<EntityEvent>) {
        let (damage, _) = self.mobs[attacker].kind.melee();
        let chance = match self.mobs[attacker].difficulty {
            Difficulty::Peaceful | Difficulty::Easy => 0.0,
            Difficulty::Normal => 0.5,
            Difficulty::Hard => 1.0,
        };
        let killing = self.mobs[victim].health <= damage && self.mobs[victim].kind == MobKind::Villager;
        if killing && self.rng.chance(chance) {
            let mob = &mut self.mobs[victim];
            mob.kind = MobKind::ZombieVillager;
            mob.health = MobKind::ZombieVillager.max_health();
            mob.dying = None;
            mob.baby = mob.age < 0;
            mob.hurt = 0.3;
            if let Some(v) = &mut mob.villager {
                v.sleeping = false;
                v.trading = false;
                v.fleeing = false;
                v.goal = None;
            }
            return;
        }
        self.hit_mob(attacker, victim, damage, 5.0, events);
    }

    /// A golden apple starts the 3–5 minute cure while Weakness is still active.
    pub fn try_cure(&mut self, index: usize) -> bool {
        let mob = &mut self.mobs[index];
        if mob.kind != MobKind::ZombieVillager || mob.weakness_left <= 0.0 || mob.convert_left > 0.0 {
            return false;
        }
        mob.convert_left = self.rng.range(180.0, 300.0);
        true
    }

    /// Natural zombies beside a villager have Java's 5% chance to be zombie villagers.
    pub(super) fn note_village_zombie(&mut self, kind: MobKind) {
        if kind != MobKind::Zombie {
            return;
        }
        let Some(pos) = self.mobs.last().map(|m| m.pos) else { return };
        let near = self
            .mobs
            .iter()
            .any(|o| o.alive() && o.kind == MobKind::Villager && o.pos.distance_squared(pos) < 64.0 * 64.0);
        if near && self.rng.chance(0.05) {
            let id = self.next_villager_id;
            self.next_villager_id = self.next_villager_id.saturating_add(1);
            let seed = self.rng.next_int(u32::MAX) as u64;
            let prof = Profession::ALL[1 + self.rng.next_int(14) as usize];
            let mob = self.mobs.last_mut().unwrap();
            if mob.baby {
                mob.age = -24000;
            }
            mob.kind = MobKind::ZombieVillager;
            let mut v = villager::Villager::new(id, seed);
            v.set_profession(prof);
            mob.villager = Some(Box::new(v));
        }
    }

    pub(super) fn hit_mob(
        &mut self,
        attacker: usize,
        victim: usize,
        damage: f32,
        up: f64,
        events: &mut Vec<EntityEvent>,
    ) {
        let pos = self.mobs[attacker].pos;
        let dir = (self.mobs[victim].pos - pos).normalize_or_zero();
        let at = self.mobs[victim].pos;
        let victim_kind = self.mobs[victim].kind;
        if self.mobs[victim].damage(damage, Some(dir * 6.0 + DVec3::Y * up), &mut self.rng) {
            events.push(EntityEvent::MobKilled {
                kind: victim_kind,
                pos: at,
                burning: self.mobs[victim].burning,
                player_kill: false,
                looting: 0,
            });
        }
    }

    /// Once a second: summon village golems and let snow golems throw.
    pub(super) fn life_tick<W: MobWorld + ?Sized>(&mut self, world: &W) {
        self.golem_calm = (self.golem_calm - 1.0).max(0.0);
        self.gossip_timer = (self.gossip_timer - 1.0).max(0.0);
        self.summon_golems(world);
        self.snow_golems(world);
    }

    fn summon_golems<W: MobWorld + ?Sized>(&mut self, world: &W) {
        if self.golem_calm > 0.0 {
            return;
        }
        let gossip = self.gossip_timer <= 0.0;
        if gossip {
            self.gossip_timer = 60.0;
        }
        let n = self.mobs.len();
        for i in 0..n {
            let m = &self.mobs[i];
            if !m.alive() || m.kind != MobKind::Villager || m.age < 0 {
                continue;
            }
            let fleeing = m.villager.as_ref().is_some_and(|v| v.fleeing);
            if !fleeing && !gossip {
                continue;
            }
            let pos = m.pos;
            let mut villagers = 0u32;
            let mut golems = 0u32;
            for o in &self.mobs {
                if !o.alive() {
                    continue;
                }
                let d = o.pos.distance_squared(pos);
                if d >= GOLEM_RANGE * GOLEM_RANGE {
                    continue;
                }
                if o.kind == MobKind::Villager && o.age >= 0 {
                    villagers += 1;
                } else if o.kind == MobKind::IronGolem {
                    golems += 1;
                }
            }
            if golems > 0 {
                continue;
            }
            let mut beds = 0u32;
            world.village_pois(&mut |p, b| {
                if b.is_bed_head()
                    && pos.distance_squared(p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)) < GOLEM_RANGE * GOLEM_RANGE
                {
                    beds += 1;
                }
            });
            let panic = fleeing && villagers >= 3 && beds >= 3;
            let chat = gossip && !fleeing && villagers >= 5 && beds >= 5 && self.rng.chance(0.1);
            if panic || chat {
                self.spawn(MobKind::IronGolem, pos);
                self.golem_calm = 30.0;
                return;
            }
        }
    }

    fn snow_golems<W: MobWorld + ?Sized>(&mut self, world: &W) {
        let n = self.mobs.len();
        for i in 0..n {
            if !self.mobs[i].alive() || self.mobs[i].kind != MobKind::SnowGolem {
                continue;
            }
            let pos = self.mobs[i].pos;
            let p = pos.floor().as_ivec3();
            if world.biome(p.x, p.z) == crate::world::terrain::Biome::Desert {
                let at = self.mobs[i].pos;
                if self.mobs[i].damage(1.0, None, &mut self.rng) {
                    self.drop_loot_with_fire(MobKind::SnowGolem, at, 0, false, false);
                }
                if !self.mobs[i].alive() {
                    continue;
                }
            }
            let mut target: Option<DVec3> = None;
            let mut best = 100.0f64;
            for o in &self.mobs {
                if o.alive() && o.kind.is_hostile() {
                    let d = o.pos.distance_squared(pos);
                    if d < best {
                        best = d;
                        target = Some(o.pos + DVec3::Y);
                    }
                }
            }
            let Some(at) = target else { continue };
            let eye = pos + DVec3::Y * 1.2;
            let snow =
                Thrown::launch(thrown::Kind::Snowball, PlayerId::HOST, eye, at - eye, DVec3::ZERO, &mut self.rng);
            self.thrown.push(snow);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Mob, Rng, Target};
    use crate::physics::BlockSource;
    use crate::physics::test_util::Grid;
    use crate::world::terrain::Dimension;

    fn ctx(player: DVec3, targetable: bool) -> Ctx {
        Ctx {
            players: vec![Target::new(PlayerId::HOST, player, targetable)],
            daylight: 1.0,
            spawning: false,
            raining: false,
            dimension: Dimension::Overworld,
        }
    }

    struct Beds {
        grid: Grid,
        beds: Vec<IVec3>,
    }
    impl BlockSource for Beds {
        fn block(&self, p: IVec3) -> Option<Block> {
            self.grid.block(p)
        }
    }
    impl MobWorld for Beds {
        fn loaded(&self, _: IVec3) -> bool {
            true
        }
        fn surface(&self, _: i32, _: i32) -> Option<i32> {
            Some(0)
        }
        fn exposed(&self, _: IVec3) -> bool {
            true
        }
        fn village_pois(&self, visit: &mut dyn FnMut(IVec3, Block)) {
            for p in &self.beds {
                visit(*p, Block::BED_HEAD);
            }
        }
    }

    #[test]
    fn patterns_match_java_and_reject_partial_builds() {
        let mut g = Grid::flat(0);
        let head = IVec3::new(0, 3, 0);
        g.set(head, Block::CARVED_PUMPKIN.with_facing(Facing::South));
        g.set(head - IVec3::Y, Block::IRON_BLOCK);
        g.set(head - IVec3::Y * 2, Block::IRON_BLOCK);
        assert!(pattern_at(&g, head).is_none(), "arms missing");
        g.set(head - IVec3::Y + IVec3::X, Block::IRON_BLOCK);
        g.set(head - IVec3::Y - IVec3::X, Block::IRON_BLOCK);
        let pattern = pattern_at(&g, head).unwrap();
        let (kind, yaw, feet) = spawn_feet(&pattern);
        assert_eq!(kind, MobKind::IronGolem);
        assert!((yaw - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!((feet.y - 1.0).abs() < 1e-6);
        g.set(head, Block::JACK_O_LANTERN);
        assert!(matches!(pattern_at(&g, head), Some(Pattern::Iron { .. })));

        let mut snow = Grid::flat(0);
        let top = IVec3::new(2, 4, 0);
        snow.set(top, Block::CARVED_PUMPKIN);
        snow.set(top - IVec3::Y, Block::SNOW);
        assert!(pattern_at(&snow, top).is_none());
        snow.set(top - IVec3::Y * 2, Block::SNOW);
        assert!(matches!(pattern_at(&snow, top), Some(Pattern::Snow { .. })));
        assert!(pattern_at(&snow, top - IVec3::Y).is_none());
    }

    #[test]
    fn iron_golem_has_one_hundred_health_and_drops_iron_and_poppies() {
        assert_eq!(MobKind::IronGolem.max_health(), 100.0);
        assert_eq!(MobKind::SnowGolem.max_health(), 4.0);
        assert!(!MobKind::IronGolem.spawns_in(Dimension::Overworld));
        let mut rng = Rng::new(3);
        let mut iron = false;
        let mut poppy = false;
        for _ in 0..30 {
            for (item, n) in MobKind::IronGolem.drops(&mut rng, 0) {
                if item == crate::item::Item::IRON_INGOT {
                    assert!((3..=5).contains(&n));
                    iron = true;
                }
                if item == crate::item::Item::from(Block::POPPY) {
                    assert!((1..=2).contains(&n));
                    poppy = true;
                }
            }
        }
        assert!(iron && poppy);
        assert_eq!(iron_damage(Difficulty::Normal, true), 15.0);
        assert_eq!(iron_damage(Difficulty::Easy, false), 7.5);
        assert_eq!(iron_damage(Difficulty::Peaceful, true), 0.0);
        assert_eq!(iron_damage(Difficulty::Hard, false), 22.5);
    }

    #[test]
    fn golem_knocks_a_zombie_up_while_a_villager_is_near() {
        let world = Grid::flat(0);
        let mut e = Entities::new(1);
        e.spawn(MobKind::IronGolem, DVec3::new(0.5, 1.0, 0.5));
        e.spawn(MobKind::Zombie, DVec3::new(2.0, 1.0, 0.5));
        e.spawn(MobKind::Villager, DVec3::new(4.0, 1.0, 0.5));
        let events = e.update(0.05, &world, &ctx(DVec3::new(40.0, 1.0, 0.5), false));
        assert!(e.mobs[1].health < MobKind::Zombie.max_health(), "zombie health {}", e.mobs[1].health);
        assert!(e.mobs[1].vel.y >= 8.0, "knockup {}", e.mobs[1].vel.y);
        assert!(events.iter().all(|ev| !matches!(ev, EntityEvent::PlayerHit { .. })));
        let before = e.mobs[0].health;
        e.mobs[0].vel = DVec3::ZERO;
        e.attack(0, DVec3::X, 1.0);
        assert_eq!(e.mobs[0].health, before - 1.0);
        assert_eq!(e.mobs[0].vel, DVec3::ZERO, "iron golems ignore knockback");
        let player = ctx(DVec3::new(1.2, 1.0, 0.5), true);
        let mut angry = Vec::new();
        for _ in 0..40 {
            angry.extend(e.update(0.05, &world, &player));
        }
        assert!(angry.iter().any(|ev| matches!(ev, EntityEvent::PlayerHit { damage, .. } if *damage == 15.0)));
    }

    #[test]
    fn panicking_villagers_summon_one_golem_beside_enough_beds() {
        let world =
            Beds { grid: Grid::flat(0), beds: vec![IVec3::new(1, 1, 0), IVec3::new(2, 1, 0), IVec3::new(3, 1, 0)] };
        let mut e = Entities::new(2);
        for x in 0..3 {
            e.spawn(MobKind::Villager, DVec3::new(x as f64 + 0.5, 1.0, 0.5));
            e.mobs[x].villager.as_mut().unwrap().fleeing = true;
        }
        e.life_tick(&world);
        assert_eq!(e.count(MobKind::IronGolem), 1);
        e.life_tick(&world);
        assert_eq!(e.count(MobKind::IronGolem), 1, "30s calm after a summon");
        for mob in &mut e.mobs {
            if let Some(v) = &mut mob.villager {
                v.fleeing = false;
            }
        }
        e.golem_calm = 0.0;
        e.mobs.retain(|m| m.kind != MobKind::IronGolem);
        e.life_tick(&world);
        assert_eq!(e.count(MobKind::IronGolem), 0, "calm villagers do not summon on a panic check");
    }

    #[test]
    fn a_hit_does_not_shove_an_iron_golem() {
        let mut mob = Mob::new(MobKind::IronGolem, DVec3::new(0.5, 1.0, 0.5), 0.0);
        let mut rng = Rng::new(1);
        assert!(!mob.damage(10.0, Some(DVec3::new(6.0, 5.0, 0.0)), &mut rng));
        assert_eq!(mob.health, 90.0);
        assert_eq!(mob.vel, DVec3::ZERO);
        assert_ne!(mob.ai, crate::entity::mob::Ai::Panic);
    }

    #[test]
    fn a_hard_zombie_converts_a_villager_and_keeps_the_profession() {
        let mut e = Entities::new(4);
        e.spawn(MobKind::Zombie, DVec3::new(0.5, 1.0, 0.5));
        e.spawn(MobKind::Villager, DVec3::new(1.6, 1.0, 0.5));
        e.mobs[0].difficulty = Difficulty::Hard;
        e.mobs[1].villager.as_mut().unwrap().set_profession(Profession::Farmer);
        let offers = e.mobs[1].villager.as_ref().unwrap().offers;
        e.mobs[1].health = 1.0;
        e.mobs[0].strike = true;
        let mut events = Vec::new();
        e.resolve_strikes(&ctx(DVec3::new(40.0, 1.0, 0.5), false), &mut events);
        assert_eq!(e.mobs[1].kind, MobKind::ZombieVillager);
        assert_eq!(e.mobs[1].health, MobKind::ZombieVillager.max_health());
        assert_eq!(e.mobs[1].villager.as_ref().unwrap().profession, Profession::Farmer);
        assert_eq!(e.mobs[1].villager.as_ref().unwrap().offers, offers);
        assert!(events.iter().all(|ev| !matches!(ev, EntityEvent::MobKilled { .. })));

        e.mobs[1].kind = MobKind::Villager;
        e.mobs[1].health = 1.0;
        e.mobs[0].difficulty = Difficulty::Easy;
        e.mobs[0].strike = true;
        events.clear();
        e.resolve_strikes(&ctx(DVec3::new(40.0, 1.0, 0.5), false), &mut events);
        assert_eq!(e.mobs[1].kind, MobKind::Villager);
        assert!(events.iter().any(|ev| matches!(ev, EntityEvent::MobKilled { .. })));
    }

    #[test]
    fn weakness_and_a_golden_apple_cure_into_a_discount() {
        let world = Grid::flat(1);
        let mut e = Entities::new(9);
        e.spawn(MobKind::Villager, DVec3::new(0.5, 1.0, 0.5));
        e.mobs[0].kind = MobKind::ZombieVillager;
        e.mobs[0].villager.as_mut().unwrap().set_profession(Profession::Cleric);
        assert!(!e.try_cure(0), "a golden apple does nothing without weakness");
        e.mobs[0].weakness_left = 30.0;
        assert!(e.try_cure(0));
        assert!((180.0..=300.0).contains(&e.mobs[0].convert_left));
        assert!(!e.try_cure(0), "a second apple does not restart the cure");
        e.mobs[0].convert_left = 0.05;
        e.update(0.05, &world, &ctx(DVec3::new(30.0, 1.0, 0.5), false));
        assert_eq!(e.mobs[0].kind, MobKind::Villager);
        let v = e.mobs[0].villager.as_ref().unwrap();
        assert_eq!(v.reputation, 100);
        assert_eq!(v.profession, Profession::Cleric);
        let offer = v.offers.iter().copied().flatten().next().unwrap();
        assert!(v.priced(offer).count <= offer.price().count);
        assert!(v.priced(offer).count >= 1);
    }

    #[test]
    fn some_zombies_near_villagers_spawn_as_zombie_villagers() {
        let mut e = Entities::new(2);
        e.spawn(MobKind::Villager, DVec3::new(0.5, 1.0, 0.5));
        let mut converted = 0;
        for _ in 0..200 {
            e.spawn(MobKind::Zombie, DVec3::new(2.0, 1.0, 0.5));
            e.note_village_zombie(MobKind::Zombie);
            let mob = e.mobs.last().unwrap();
            if mob.kind == MobKind::ZombieVillager {
                converted += 1;
                assert_ne!(mob.villager.as_ref().unwrap().profession, Profession::None);
            }
            e.mobs.pop();
        }
        assert!((2..40).contains(&converted), "{converted}");
        e.spawn(MobKind::Zombie, DVec3::new(200.0, 1.0, 0.5));
        e.note_village_zombie(MobKind::Zombie);
        assert_eq!(e.mobs.last().unwrap().kind, MobKind::Zombie);
    }
}
