//! Village life beyond residents: golem patterns, summons, and who they hit.
//!
//! Summoning needs recent sleep, three panicking or five gossiping villagers,
//! no recently detected golem, and a supported unobstructed spawn position.
//!
//! Player-built golems use Java's patterns (the pumpkin is placed last).
//! Iron: a T of iron blocks under the pumpkin, arms on either horizontal
//! axis. Snow: two snow blocks under the pumpkin. Jack o'lanterns count.
use glam::{DVec3, IVec3};

use super::thrown::{self, Thrown};
#[cfg(test)]
use super::villager::Profession;
use super::{Ctx, Entities, EntityEvent, MobKind, MobWorld, PlayerId};
#[cfg(test)]
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
            if world.block(neck + a) == Some(Block::IRON_BLOCK)
                && world.block(neck + b) == Some(Block::IRON_BLOCK)
                && [pos + a, pos + b, hip + a, hip + b].into_iter().all(|p| world.block(p) == Some(Block::AIR))
            {
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

/// Java rolls 7.5–21.5 raw damage. PlayerHit events carry unscaled
/// damage; the app applies difficulty once, then armor/hurt immunity.
pub fn iron_damage(rng: &mut super::Rng) -> f32 {
    7.5 + rng.next_int(15) as f32
}

fn anger_target<'a>(mob: &super::Mob, ctx: &'a Ctx) -> Option<&'a super::Target> {
    if let Some(owner) = mob.angry_player {
        ctx.players.iter().find(|p| p.id == owner && p.alive && p.targetable)
    } else {
        ctx.nearest_target(mob.pos)
    }
}

const GOLEM_RANGE: f64 = 16.0;

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
    /// Built golems never retaliate against players.
    pub(super) fn assign_hunts(&mut self, ctx: &Ctx) {
        self.mob_index.rebuild(&self.mobs);
        for i in 0..self.mobs.len() {
            let m = &self.mobs[i];
            let pos = m.pos;
            let hunt = if !m.alive() {
                None
            } else {
                match m.kind {
                    MobKind::IronGolem if m.angry_at_player() => anger_target(m, ctx)
                        .filter(|p| p.pos.distance_squared(pos) < GOLEM_RANGE * GOLEM_RANGE)
                        .map(|p| p.pos),
                    MobKind::IronGolem => self
                        .mob_index
                        .nearest(&self.mobs, pos, GOLEM_RANGE, |o| o.kind.is_hostile() && o.kind != MobKind::Creeper)
                        .map(|j| self.mobs[j].pos),
                    MobKind::Zombie | MobKind::Husk | MobKind::Drowned | MobKind::ZombieVillager => self
                        .mob_index
                        .nearest(&self.mobs, pos, GOLEM_RANGE, |o| o.kind == MobKind::Villager)
                        .filter(|&j| {
                            !ctx.nearest_target(pos)
                                .is_some_and(|p| p.pos.distance_squared(pos) < self.mobs[j].pos.distance_squared(pos))
                        })
                        .map(|j| self.mobs[j].pos),
                    MobKind::WanderingTrader => self
                        .mob_index
                        .nearest(&self.mobs, pos, 8.0, |m| m.kind.is_hostile())
                        .map(|j| pos + (pos - self.mobs[j].pos).normalize_or_zero() * 10.0),
                    // Java creepers do not flee golems (they flee cats/ocelots).
                    _ => None,
                }
            };
            self.mobs[i].hunt = hunt;
            self.mobs[i].strike = false;
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
            if kind == MobKind::IronGolem
                && self.mobs[i].angry_at_player()
                && let Some(player) = anger_target(&self.mobs[i], ctx)
                && player.pos.distance_squared(pos) < 9.0
            {
                let dir = (player.pos - pos).normalize_or_zero();
                events.push(EntityEvent::PlayerHit {
                    player: player.id,
                    damage: iron_damage(&mut self.rng),
                    knockback: (dir * 6.0 + DVec3::Y * 8.0).as_vec3(),
                    cause: "was slain by an iron golem",
                });
                continue;
            }
            if kind.is_zombie() {
                let bitten = self.mob_index.nearest(&self.mobs, pos, 1.5, |m| m.kind == MobKind::Villager);
                if let Some(j) = bitten {
                    self.infect_or_hit(i, j, events);
                }
                continue;
            }
            let victim = (kind == MobKind::IronGolem)
                .then(|| {
                    self.mob_index.nearest(&self.mobs, pos, 3.0, |m| m.kind.is_hostile() && m.kind != MobKind::Creeper)
                })
                .flatten();
            let Some(j) = victim else { continue };
            let damage = iron_damage(&mut self.rng);
            self.hit_mob(i, j, damage, 8.0, events);
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
        self.mob_index.rebuild(&self.mobs);
        for i in 0..self.mobs.len() {
            if self.mobs[i].kind != MobKind::Villager || !self.mobs[i].alive() {
                continue;
            }
            let seen = self
                .mob_index
                .nearest(&self.mobs, self.mobs[i].pos, GOLEM_RANGE, |m| m.kind == MobKind::IronGolem)
                .is_some();
            let v = self.mobs[i].villager.as_mut().unwrap();
            if v.active {
                v.golem_seen = if seen { 30.0 } else { (v.golem_seen - 1.0).max(0.0) };
            }
        }
        self.gossip_timer = (self.gossip_timer - 1.0).max(0.0);
        if self.gossip_timer <= 0.0 {
            self.share_gossip();
        }
        self.summon_golems(world);
        self.snow_golems(world);
    }

    fn summon_golems<W: MobWorld + ?Sized>(&mut self, world: &W) {
        let gossip = self.gossip_timer <= 0.0;
        if gossip {
            self.gossip_timer = 60.0;
        }
        let n = self.mobs.len();
        for i in 0..n {
            let m = &self.mobs[i];
            let now = self.village_day * 24000 + (self.village_time * 24000.0) as i64;
            let eligible_villager = |o: &super::Mob| {
                o.alive()
                    && o.kind == MobKind::Villager
                    && o.age >= 0
                    && o.villager.as_ref().is_some_and(|v| {
                        v.active && v.golem_seen <= 0.0 && v.last_slept.is_some_and(|t| (0..24000).contains(&(now - t)))
                    })
            };
            if !eligible_villager(m) {
                continue;
            }
            let fleeing = m.villager.as_ref().is_some_and(|v| v.fleeing);
            if !fleeing && !gossip {
                continue;
            }
            let pos = m.pos;

            let mut eligible = 0;
            self.mob_index.visit(pos, 10.0, |j| {
                let o = &self.mobs[j];
                if eligible_villager(o) && o.pos.distance_squared(pos) <= 100.0 {
                    eligible += 1;
                }
            });
            let panic = fleeing && eligible >= 3;
            let chat = gossip && !fleeing && eligible >= 5;
            if panic || chat {
                let mut spawned = false;
                'attempts: for _ in 0..10 {
                    let x = pos.x.floor() as i32 + self.rng.next_int(17) as i32 - 8;
                    let z = pos.z.floor() as i32 + self.rng.next_int(17) as i32 - 8;
                    for y in ((pos.y.floor() as i32 - 6)..=(pos.y.floor() as i32 + 6)).rev() {
                        let feet = DVec3::new(x as f64 + 0.5, y as f64, z as f64 + 0.5);
                        if world.loaded(feet.floor().as_ivec3())
                            && world.block(IVec3::new(x, y - 1, z)).is_some_and(|b| b.is_opaque())
                            && !crate::physics::overlaps_solid(world, feet, MobKind::IronGolem.shape())
                        {
                            self.spawn(MobKind::IronGolem, feet);
                            spawned = true;
                            break 'attempts;
                        }
                    }
                }
                if spawned {
                    self.mob_index.visit(pos, GOLEM_RANGE, |j| {
                        let o = &mut self.mobs[j];
                        if o.pos.distance_squared(pos) <= GOLEM_RANGE * GOLEM_RANGE
                            && o.kind == MobKind::Villager
                            && let Some(v) = &mut o.villager
                        {
                            v.golem_seen = 30.0;
                        }
                    });
                }
            }
        }
    }

    fn snow_golems<W: MobWorld + ?Sized>(&mut self, world: &W) {
        let n = self.mobs.len();
        for i in 0..n {
            if !self.mobs[i].alive()
                || self.mobs[i].kind != MobKind::SnowGolem
                || !world.loaded(self.mobs[i].pos.floor().as_ivec3())
            {
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
            let target = self
                .mob_index
                .nearest(&self.mobs, pos, 10.0, |m| m.kind.is_hostile())
                .map(|j| self.mobs[j].pos + DVec3::Y);
            let Some(at) = target else { continue };
            let eye = pos + DVec3::Y * 1.2;
            let mut snow =
                Thrown::launch(thrown::Kind::Snowball, PlayerId::HOST, eye, at - eye, DVec3::ZERO, &mut self.rng);
            snow.owner = None;
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
        for _ in 0..100 {
            let damage = iron_damage(&mut rng);
            assert!((7.5..=21.5).contains(&damage));
            assert_eq!(Difficulty::Peaceful.mob_damage(damage), 0.0);
        }
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
        assert!(
            angry.iter().any(|ev| matches!(ev, EntityEvent::PlayerHit { damage, .. } if (7.5..=21.5).contains(damage)))
        );
    }

    #[test]
    fn iron_golem_player_event_is_scaled_once_by_the_app() {
        let mut e = Entities::new(1);
        e.spawn(MobKind::IronGolem, DVec3::new(0.5, 1.0, 0.5));
        e.attack(0, DVec3::X, 1.0);
        let events = e.update_difficulty(0.05, &Grid::flat(0), &ctx(DVec3::new(1.2, 1.0, 0.5), true), Difficulty::Hard);
        let raw = events
            .iter()
            .find_map(|event| match event {
                EntityEvent::PlayerHit { damage, .. } => Some(*damage),
                _ => None,
            })
            .expect("angry golem must hit the nearby player");
        assert!((7.5..=21.5).contains(&raw));
        assert!((11.25..=32.25).contains(&Difficulty::Hard.mob_damage(raw)));
    }
    #[test]
    fn panicking_villagers_summon_one_golem_beside_enough_beds() {
        let world =
            Beds { grid: Grid::flat(0), beds: vec![IVec3::new(1, 1, 0), IVec3::new(2, 1, 0), IVec3::new(3, 1, 0)] };
        let mut e = Entities::new(2);
        for x in 0..3 {
            e.spawn(MobKind::Villager, DVec3::new(x as f64 + 0.5, 1.0, 0.5));
            e.mobs[x].villager.as_mut().unwrap().fleeing = true;
            e.mobs[x].villager.as_mut().unwrap().last_slept = Some(0);
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
        e.mobs.retain(|m| m.kind != MobKind::IronGolem);
        e.life_tick(&world);
        assert_eq!(e.count(MobKind::IronGolem), 0, "calm villagers do not summon on a panic check");
    }

    #[test]
    fn distant_villages_summon_independently_and_remember_detected_golems() {
        let world = Beds { grid: Grid::flat(0), beds: vec![] };
        let mut e = Entities::new(2);
        for offset in [0.0, 80.0] {
            for x in 0..3 {
                e.spawn(MobKind::Villager, DVec3::new(offset + x as f64, 1.0, 0.0));
                let v = e.mobs.last_mut().unwrap().villager.as_mut().unwrap();
                v.last_slept = Some(0);
                v.fleeing = true;
            }
        }
        e.life_tick(&world);
        assert_eq!(e.count(MobKind::IronGolem), 2);
        e.mobs.retain(|m| m.kind != MobKind::IronGolem);
        let mut restored = Entities::new(8);
        restored.load_villagers(&e.villagers_to_string());
        // Panic is sensed again after loading; only the golem memory persists.
        for m in &mut restored.mobs {
            m.villager.as_mut().unwrap().fleeing = true;
        }
        restored.life_tick(&world);
        assert_eq!(restored.count(MobKind::IronGolem), 0);
        for _ in 0..29 {
            restored.life_tick(&world);
        }
        assert_eq!(restored.count(MobKind::IronGolem), 2);
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
        e.mob_index.rebuild(&e.mobs);
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
        e.mob_index.rebuild(&e.mobs);
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
        assert_eq!(v.gossip.reputation(PlayerId::HOST), 125);
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
            e.note_zombie_villager(MobKind::Zombie);
            let mob = e.mobs.last().unwrap();
            if mob.kind == MobKind::ZombieVillager {
                converted += 1;
                assert_ne!(mob.villager.as_ref().unwrap().profession, Profession::None);
            }
            e.mobs.pop();
        }
        assert!((2..40).contains(&converted), "{converted}");
        e.spawn(MobKind::Zombie, DVec3::new(200.0, 1.0, 0.5));
        e.note_zombie_villager(MobKind::Zombie);
        // Natural variants are also allowed far from villages.
    }
}
