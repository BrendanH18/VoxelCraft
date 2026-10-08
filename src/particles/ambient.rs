//! Client animation ticks and environmental emitters. Random block sampling
//! mirrors ClientLevel's 667 attempts at both 16- and 32-block ranges.

use super::types::random_vec;
use super::{Burst, Kind, Setting, System};
use crate::entity::{Entities, MobKind};
use crate::simulation::{
    effects::Effects,
    weather::{Precipitation, precipitation},
};
use crate::world::nether_biome::{Ambient, NetherBiome};
use crate::world::{World, block::Block, nether_biome_blocks::SOUL_TORCH};
use glam::{DVec2, DVec3, IVec2, IVec3};

/// Quarts each side of the camera's quart: the 32-block sampling range.
const BIOME_REACH: i32 = 9;
const BIOME_WIDTH: usize = 2 * BIOME_REACH as usize + 1;

/// Nether biomes of the quarts around the camera, refreshed when it moves
/// to another quart, so ambient sampling never runs the biome noise per
/// block.
pub struct BiomeWindow {
    center: Option<IVec2>,
    biomes: [[NetherBiome; BIOME_WIDTH]; BIOME_WIDTH],
}

impl Default for BiomeWindow {
    fn default() -> Self {
        Self { center: None, biomes: [[NetherBiome::NetherWastes; BIOME_WIDTH]; BIOME_WIDTH] }
    }
}

impl BiomeWindow {
    fn update(&mut self, world: &World, camera: IVec3) {
        let center = IVec2::new(camera.x, camera.z).div_euclid(IVec2::splat(4));
        if self.center == Some(center) {
            return;
        }
        self.center = Some(center);
        for (dz, row) in self.biomes.iter_mut().enumerate() {
            for (dx, b) in row.iter_mut().enumerate() {
                let q = center + IVec2::new(dx as i32, dz as i32) - BIOME_REACH;
                *b = world.generator.nether_biome(q.x * 4, q.y * 4).unwrap_or(NetherBiome::NetherWastes);
            }
        }
    }

    fn at(&self, cell: IVec3) -> Option<NetherBiome> {
        let l = IVec2::new(cell.x, cell.z).div_euclid(IVec2::splat(4)) - self.center? + BIOME_REACH;
        let w = BIOME_WIDTH as i32;
        (l.x >= 0 && l.y >= 0 && l.x < w && l.y < w).then(|| self.biomes[l.y as usize][l.x as usize])
    }
}

impl System {
    pub fn animate_blocks(&mut self, world: &World, center: DVec3, setting: Setting) {
        if setting == Setting::Minimal {
            return;
        }
        let center = center.floor().as_ivec3();
        let nether = world.generator.dimension == crate::world::terrain::Dimension::Nether;
        if nether {
            self.biomes.update(world, center);
        }
        for _ in 0..667 {
            for range in [16, 32] {
                let mut offset =
                    || (self.rng.next_f32() * range as f32) as i32 - (self.rng.next_f32() * range as f32) as i32;
                let cell = center + IVec3::new(offset(), offset(), offset());
                self.animate_block(world, cell, setting);
                if nether {
                    self.biome_ambient(world, cell, setting);
                }
            }
        }
    }

    /// `ClientLevel.doAnimateTick`'s biome particles: in any cell that isn't
    /// a full block, the biome's ambient particle with its probability.
    fn biome_ambient(&mut self, world: &World, cell: IVec3, setting: Setting) {
        let Some((ambient, chance)) = self.biomes.at(cell).and_then(NetherBiome::ambient) else { return };
        if !self.rng.chance(chance) || world.get_block(cell).is_none_or(|b| b.is_opaque()) {
            return;
        }
        let pos = cell.as_dvec3() + random_vec(&mut self.rng);
        let mut burst = match ambient {
            Ambient::CrimsonSpore => Burst::new(Kind::CrimsonSpore, pos, 1),
            Ambient::WarpedSpore => Burst::new(Kind::WarpedSpore, pos, 1),
            Ambient::Ash => Burst::new(Kind::Ash, pos, 1),
            Ambient::WhiteAsh => Burst::new(Kind::WhiteAsh, pos, 1),
        };
        if ambient == Ambient::WhiteAsh {
            // WhiteAshParticle.Provider: a drift down and against +x/+z.
            let mut drift = || self.rng.next_f32() as f64 * self.rng.next_f32() as f64;
            let d = DVec2::new(drift() * -0.19, drift() * -0.19);
            burst.velocity = DVec3::new(d.x, drift() * -0.25, d.y);
        }
        self.burst(burst, setting);
    }

    pub fn animate_block(&mut self, world: &World, cell: IVec3, setting: Setting) {
        let Some(block) = world.get_block(cell) else { return };
        let base = cell.as_dvec3();
        if block == Block::TORCH || block == SOUL_TORCH {
            let pos = base + DVec3::new(0.5, 0.7, 0.5);
            let flame = if block == SOUL_TORCH { Kind::SoulFlame } else { Kind::Flame };
            self.burst(Burst::new(Kind::Smoke, pos, 1), setting);
            self.burst(Burst::new(flame, pos, 1), setting);
        } else if block.is_fire() {
            self.fire_smoke(world, cell, setting);
        } else if block.is_lava() {
            if world.get_block(cell + IVec3::Y) == Some(Block::AIR) && self.rng.chance(0.01) {
                let mut pos = base + random_vec(&mut self.rng);
                pos.y = base.y + 1.0;
                self.burst(Burst::new(Kind::Lava, pos, 1), setting);
            }
            if self.rng.chance(0.1)
                && world.get_block(cell - IVec3::Y).is_some_and(|b| b.is_opaque() && b.height() == 1.0)
                && world.get_block(cell - IVec3::Y * 2) == Some(Block::AIR)
            {
                let mut pos = base + random_vec(&mut self.rng);
                pos.y = base.y - 1.05;
                self.burst(Burst::new(Kind::LavaDrip, pos, 1), setting);
            }
        } else if block == Block::NETHER_PORTAL {
            for _ in 0..4 {
                let mut pos = base + random_vec(&mut self.rng);
                let mut velocity = (random_vec(&mut self.rng) - DVec3::splat(0.5)) * 0.5;
                let sign = if self.rng.chance(0.5) { -1.0 } else { 1.0 };
                let axis = if world.get_block(cell - IVec3::X) != Some(block)
                    && world.get_block(cell + IVec3::X) != Some(block)
                {
                    0
                } else {
                    2
                };
                pos[axis] = base[axis] + 0.5 + 0.25 * sign;
                velocity[axis] = self.rng.next_f32() as f64 * 2.0 * sign;
                let mut b = Burst::new(Kind::Portal, pos, 1);
                b.velocity = velocity;
                self.burst(b, setting);
            }
        } else if block == Block::END_PORTAL {
            // Java End portals produce smoke rather than Nether-style purple particles.
            let mut pos = base + random_vec(&mut self.rng);
            pos.y = base.y + 0.8;
            self.burst(Burst::new(Kind::Smoke, pos, 1), setting);
        } else if block == Block::END_GATEWAY {
            self.gateway(world, cell, setting);
        } else if block == Block::ENCHANTING_TABLE {
            for y in 0..=1 {
                for z in -2..=2i32 {
                    for x in -2..=2i32 {
                        if x.abs() != 2 && z.abs() != 2 {
                            continue;
                        }
                        if self.rng.chance(1.0 / 16.0)
                            && world.get_block(cell + IVec3::new(x, y, z)) == Some(Block::BOOKSHELF)
                            && world.get_block(cell + IVec3::new(x / 2, y, z / 2)).is_some_and(Block::is_replaceable)
                        {
                            let mut b = Burst::new(Kind::Glyph, base + DVec3::new(0.5, 2.0, 0.5), 1);
                            b.velocity = DVec3::new(
                                x as f64 + self.rng.next_f32() as f64 - 0.5,
                                y as f64 - self.rng.next_f32() as f64 - 1.0,
                                z as f64 + self.rng.next_f32() as f64 - 0.5,
                            );
                            self.burst(b, setting);
                        }
                    }
                }
            }
        }
    }

    /// Large smoke: three rising puffs when the block below can burn or has
    /// a solid top, otherwise two puffs on each flammable face.
    fn fire_smoke(&mut self, world: &World, cell: IVec3, setting: Setting) {
        let base = cell.as_dvec3();
        let below = world.get_block(cell - IVec3::Y).unwrap_or(Block::AIR);
        if below.fire_odds().0 > 0 || below.supports_fire() {
            for _ in 0..3 {
                let pos = base + DVec3::new(self.unit(), self.unit() * 0.5 + 0.5, self.unit());
                self.burst(Burst::new(Kind::LargeSmoke, pos, 1), setting);
            }
            return;
        }
        let burn = |side: IVec3| world.get_block(cell + side).is_some_and(|b| b.fire_odds().0 > 0);
        if burn(IVec3::NEG_X) {
            for _ in 0..2 {
                let pos = DVec3::new(base.x + self.unit() * 0.1, base.y + self.unit(), base.z + self.unit());
                self.burst(Burst::new(Kind::LargeSmoke, pos, 1), setting);
            }
        }
        if burn(IVec3::X) {
            for _ in 0..2 {
                let pos = DVec3::new(base.x + 1.0 - self.unit() * 0.1, base.y + self.unit(), base.z + self.unit());
                self.burst(Burst::new(Kind::LargeSmoke, pos, 1), setting);
            }
        }
        if burn(IVec3::NEG_Z) {
            for _ in 0..2 {
                let pos = DVec3::new(base.x + self.unit(), base.y + self.unit(), base.z + self.unit() * 0.1);
                self.burst(Burst::new(Kind::LargeSmoke, pos, 1), setting);
            }
        }
        if burn(IVec3::Z) {
            for _ in 0..2 {
                let pos = DVec3::new(base.x + self.unit(), base.y + self.unit(), base.z + 1.0 - self.unit() * 0.1);
                self.burst(Burst::new(Kind::LargeSmoke, pos, 1), setting);
            }
        }
        if burn(IVec3::Y) {
            for _ in 0..2 {
                let pos = DVec3::new(base.x + self.unit(), base.y + 1.0 - self.unit() * 0.1, base.z + self.unit());
                self.burst(Burst::new(Kind::LargeSmoke, pos, 1), setting);
            }
        }
    }

    /// One portal particle per exposed face, aimed out of a random side.
    fn gateway(&mut self, world: &World, cell: IVec3, setting: Setting) {
        let exposed = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z]
            .into_iter()
            .filter(|side| !world.get_block(cell + *side).is_some_and(|b| b == Block::END_GATEWAY || b.is_opaque()))
            .count();
        let base = cell.as_dvec3();
        for _ in 0..exposed {
            let mut pos = base + random_vec(&mut self.rng);
            let mut velocity = (random_vec(&mut self.rng) - DVec3::splat(0.5)) * 0.5;
            let sign = if self.rng.chance(0.5) { -1.0 } else { 1.0 };
            if self.rng.chance(0.5) {
                pos.z = base.z + 0.5 + 0.25 * sign;
                velocity.z = self.rng.next_f32() as f64 * 2.0 * sign;
            } else {
                pos.x = base.x + 0.5 + 0.25 * sign;
                velocity.x = self.rng.next_f32() as f64 * 2.0 * sign;
            }
            let mut b = Burst::new(Kind::Portal, pos, 1);
            b.velocity = velocity;
            self.burst(b, setting);
        }
    }

    fn unit(&mut self) -> f64 {
        self.rng.next_f32() as f64
    }

    /// Java rain uses 100 * strength² candidate columns per tick (half on
    /// Decreased), within ten blocks of the camera, and the normal limiter.
    pub fn rain(&mut self, world: &World, center: DVec3, strength: f32, setting: Setting, fancy: bool) {
        if strength <= 0.0 || setting == Setting::Minimal || !world.generator.dimension.has_sky() {
            return;
        }
        let strength = strength * if fancy { 1.0 } else { 0.5 };
        let count = (100.0 * strength * strength) as usize / if setting == Setting::Decreased { 2 } else { 1 };
        let center = center.floor().as_ivec3();
        for _ in 0..count {
            let x = center.x + (self.rng.next_f32() * 21.0) as i32 - 10;
            let z = center.z + (self.rng.next_f32() * 21.0) as i32 - 10;
            let Some(y) = world.surface_height(x, z) else { continue };
            if (y + 1 - center.y).abs() > 10 || precipitation(world, x, z, y) != Precipitation::Rain {
                continue;
            }
            let Some(block) = world.get_block(IVec3::new(x, y, z)) else { continue };
            let top = if block.is_fluid() { 1.0 - block.fluid_drop() as f64 / 16.0 } else { block.height() };
            let pos = DVec3::new(
                x as f64 + self.rng.next_f32() as f64,
                y as f64 + top + 0.001,
                z as f64 + self.rng.next_f32() as f64,
            );
            let kind = if block.is_lava()
                || block.is_fire()
                || world.get_block(IVec3::new(x, y + 1, z)).is_some_and(Block::is_fire)
            {
                Kind::Smoke
            } else {
                Kind::Rain
            };
            self.burst(Burst::new(kind, pos, 1), setting);
        }
    }

    pub fn effects(&mut self, pos: DVec3, effects: &Effects, setting: Setting) {
        if effects.is_empty() || !self.rng.chance(0.5) {
            return;
        }
        let mut color = [0.0; 4];
        let mut weight = 0.0;
        for effect in effects.iter() {
            let w = effect.amplifier as f32 + 1.0;
            for (i, channel) in effect.effect.colour().iter().enumerate() {
                color[i] += *channel as f32 / 255.0 * w;
            }
            weight += w;
        }
        for channel in &mut color[..3] {
            *channel /= weight;
        }
        color[3] = 1.0;
        let mut b = Burst::new(Kind::Effect, pos + DVec3::Y * 0.9, 1);
        b.spread = DVec3::new(0.3, 0.9, 0.3);
        b.color = Some(color);
        self.burst(b, setting);
    }

    /// Ambient mob/projectile visuals are sampled client-side, without
    /// modifying authoritative AI RNG state or creating headless particles.
    pub fn entities(&mut self, world: &World, entities: &Entities, centers: &[DVec3], setting: Setting) {
        let near = |pos: DVec3| centers.iter().any(|&center| pos.distance_squared(center) <= 32.0 * 32.0);
        for mob in &entities.mobs {
            if !near(mob.pos) || !mob.alive() {
                continue;
            }
            if mob.kind == MobKind::Enderman {
                let shape = mob.shape();
                let width = shape.half_width * 2.0;
                for _ in 0..2 {
                    let pos = mob.pos
                        + DVec3::new(
                            (self.rng.next_f32() as f64 - 0.5) * width,
                            self.rng.next_f32() as f64 * shape.height - 0.25,
                            (self.rng.next_f32() as f64 - 0.5) * width,
                        );
                    let mut b = Burst::new(Kind::Portal, pos, 1);
                    b.velocity = DVec3::new(
                        self.rng.range(-1.0, 1.0) as f64,
                        -self.rng.next_f32() as f64,
                        self.rng.range(-1.0, 1.0) as f64,
                    );
                    self.burst(b, setting);
                }
            } else if mob.kind == MobKind::Blaze {
                let mut b = Burst::new(Kind::LargeSmoke, mob.pos + DVec3::Y * mob.shape().height * 0.5, 2);
                b.spread = DVec3::new(0.3, mob.shape().height * 0.5, 0.3);
                self.burst(b, setting);
            }
        }
        for eye in &entities.eyes {
            if !near(eye.pos) {
                continue;
            }
            let velocity = eye.pos - eye.previous_pos;
            let water = world.get_block(eye.pos.floor().as_ivec3()).is_some_and(Block::is_water);
            let mut b = Burst::new(
                if water { Kind::Bubble } else { Kind::Portal },
                eye.pos - velocity * 0.25,
                if water { 4 } else { 1 },
            );
            if !water {
                b.pos.y -= 0.5;
                b.spread = DVec3::new(0.3, 0.0, 0.3);
            }
            b.velocity = velocity;
            self.burst(b, setting);
        }
        if let Some(fight) = &entities.fight {
            for cloud in &fight.clouds {
                if !near(cloud.pos) {
                    continue;
                }
                let count = (std::f32::consts::PI * cloud.radius * cloud.radius).ceil() as usize;
                for _ in 0..count.min(256) {
                    let angle = self.rng.next_f32() as f64 * std::f64::consts::TAU;
                    let radius = (self.rng.next_f32() as f64).sqrt() * cloud.radius as f64;
                    let pos = cloud.pos + DVec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);
                    let mut b = Burst::new(Kind::DragonBreath, pos, 1);
                    b.important = true;
                    b.velocity =
                        DVec3::new(self.rng.range(-0.075, 0.075) as f64, 0.01, self.rng.range(-0.075, 0.075) as f64);
                    self.burst(b, setting);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world() -> World {
        let mut w =
            World::new_headless(std::sync::Arc::new(crate::world::terrain::Generator::new(42)), Default::default(), 2);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            w.update(DVec3::new(0.5, 150.0, 0.5));
            if w.loaded_chunks() > 0 && w.pending_jobs() == 0 {
                return w;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn torches_emit_flame_and_smoke_and_minimal_filters_them() {
        let mut w = world();
        let cell = IVec3::new(0, 150, 0);
        w.set_block(cell - IVec3::Y, Block::STONE);
        w.set_block(cell, Block::TORCH);
        let mut system = System::new(1);
        system.animate_block(&w, cell, Setting::All);
        assert_eq!(system.pool.iter().count(), 2);
        assert!(system.pool.iter().any(|p| p.style == Kind::Flame));
        assert!(system.pool.iter().any(|p| p.style == Kind::Smoke));
        system.clear();
        system.animate_block(&w, cell, Setting::Minimal);
        assert_eq!(system.pool.iter().count(), 0);
    }

    #[test]
    fn water_entry_queues_bubbles_and_splashes_but_lava_does_not() {
        let mut w = world();
        let cell = IVec3::new(0, 150, 0);
        w.set_block(cell - IVec3::Y, Block::STONE);
        w.set_block(cell, Block::WATER);
        let player = crate::player::Player::new(cell.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        super::super::water_entry(&player, player.pos - DVec3::X, &mut w);
        let events: Vec<_> = w.particles.drain().collect();
        assert_eq!(
            events
                .iter()
                .filter(|r| matches!(
                    r,
                    super::super::Request::Burst(Burst { kind: Kind::Bubble | Kind::Splash, count: 13, .. })
                ))
                .count(),
            2
        );
        w.set_block(cell, Block::LAVA);
        w.particles.drain().for_each(drop);
        super::super::water_entry(&player, player.pos - DVec3::X, &mut w);
        assert_eq!(w.particles.drain().count(), 0);
    }

    #[test]
    fn fire_on_stone_emits_three_large_smokes_and_a_log_face_emits_two() {
        let mut w = world();
        let cell = IVec3::new(2, 150, 2);
        w.set_block(cell - IVec3::Y, Block::STONE);
        w.set_block(cell, Block::fire(0));
        let mut system = System::new(3);
        system.animate_block(&w, cell, Setting::All);
        assert_eq!(system.pool.iter().filter(|p| p.style == Kind::LargeSmoke).count(), 3);
        system.clear();
        for y in -1..=1 {
            for z in -1..=1 {
                for x in -1..=1 {
                    w.set_block(cell + IVec3::new(x, y, z), Block::AIR);
                }
            }
        }
        w.set_block(cell + IVec3::NEG_X, Block::LOG);
        w.set_block(cell, Block::fire(0));
        system.animate_block(&w, cell, Setting::All);
        assert_eq!(system.pool.iter().filter(|p| p.style == Kind::LargeSmoke).count(), 2);
    }

    #[test]
    fn an_exposed_end_gateway_emits_one_portal_particle_per_face() {
        let mut w = world();
        let cell = IVec3::new(4, 150, 4);
        for y in -1..=1 {
            for z in -1..=1 {
                for x in -1..=1 {
                    w.set_block(cell + IVec3::new(x, y, z), Block::AIR);
                }
            }
        }
        w.set_block(cell, Block::END_GATEWAY);
        let mut system = System::new(4);
        system.animate_block(&w, cell, Setting::All);
        assert_eq!(system.pool.iter().filter(|p| p.style == Kind::Portal).count(), 6);
        system.clear();
        system.animate_block(&w, cell, Setting::Minimal);
        assert_eq!(system.pool.iter().count(), 0);
    }

    #[test]
    fn enchanting_tables_send_glyphs_toward_bookshelves() {
        let mut w = world();
        let cell = IVec3::new(8, 150, 8);
        w.set_block(cell, Block::ENCHANTING_TABLE);
        for y in 0..=1 {
            for z in -2..=2i32 {
                for x in -2..=2i32 {
                    if x.abs() == 2 || z.abs() == 2 {
                        w.set_block(cell + IVec3::new(x, y, z), Block::BOOKSHELF);
                    }
                }
            }
        }
        let mut system = System::new(7);
        let mut glyphs = 0;
        for _ in 0..16 {
            system.animate_block(&w, cell, Setting::All);
            glyphs += system.pool.iter().filter(|p| p.style == Kind::Glyph).count();
            if glyphs > 0 {
                let glyph = system.pool.iter().find(|p| p.style == Kind::Glyph).unwrap();
                assert_eq!(glyph.motion, super::super::Motion::Glyph);
                break;
            }
            system.clear();
        }
        assert!(glyphs > 0);
        system.clear();
        for _ in 0..8 {
            system.animate_block(&w, cell, Setting::Minimal);
        }
        assert_eq!(system.pool.iter().count(), 0);
    }

    #[test]
    fn effect_swirls_use_the_potion_colour_and_minimal_rain_is_silent() {
        let mut system = System::new(5);
        let mut effects = Effects::default();
        effects.add(crate::simulation::effects::Effect::Speed, 0, 200);
        let colour = crate::simulation::effects::Effect::Speed.colour();
        let mut matched = false;
        for _ in 0..48 {
            system.clear();
            system.effects(DVec3::Y * 80.0, &effects, Setting::All);
            if let Some(p) = system.pool.iter().next() {
                assert_eq!(p.style, Kind::Effect);
                assert!((p.color[0] - colour[0] as f32 / 255.0).abs() < 1e-5);
                assert!((p.color[2] - colour[2] as f32 / 255.0).abs() < 1e-5);
                matched = true;
                break;
            }
        }
        assert!(matched);
        system.clear();
        let w = world();
        system.rain(&w, DVec3::new(0.5, 80.0, 0.5), 1.0, Setting::Minimal, true);
        assert_eq!(system.pool.iter().count(), 0);
        system.rain(&w, DVec3::new(0.5, 80.0, 0.5), 0.0, Setting::All, true);
        assert_eq!(system.pool.iter().count(), 0);
    }

    #[test]
    fn endermen_emit_two_portal_particles_near_a_camera() {
        let w = world();
        let mut entities = Entities::new(1);
        let pos = DVec3::new(0.5, 80.0, 0.5);
        entities.spawn(MobKind::Enderman, pos);
        let mut system = System::new(6);
        system.entities(&w, &entities, &[pos], Setting::All);
        assert_eq!(system.pool.iter().filter(|p| p.style == Kind::Portal).count(), 2);
        system.clear();
        system.entities(&w, &entities, &[pos + DVec3::X * 40.0], Setting::All);
        assert_eq!(system.pool.iter().count(), 0);
    }
}
