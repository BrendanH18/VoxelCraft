//! Java 26.3 CatSpawner and cat-variant selectors. No natural biome entry.
use super::{Ctx, Entities, Mob, MobKind, MobWorld, Rng};
use crate::world::{
    temples::{Built, Kind},
    terrain::Dimension,
};
use glam::{DVec3, IVec3};
use rustc_hash::FxHashSet;

pub const DELAY_TICKS: u32 = 1200;
pub const MIN_BEDS: usize = 5;
pub const VILLAGE_CAP: usize = 5;
/// Stable local appearance codes. Zero preserves the old all-black cat.
pub const VARIANTS: [&str; 11] = [
    "all_black",
    "tabby",
    "black",
    "red",
    "siamese",
    "british_shorthair",
    "calico",
    "persian",
    "ragdoll",
    "white",
    "jellie",
];
#[derive(Default)]
pub(super) struct Spawner {
    seconds: f64,
}

fn hut<W: MobWorld + ?Sized>(w: &W, p: IVec3) -> bool {
    w.structures_near(p, 0).iter().any(|b| b.kind == Kind::SwampHut && b.bounds.contains(p))
}
pub(super) fn variant<W: MobWorld + ?Sized>(w: &W, p: IVec3, moon: f32, rng: &mut Rng) -> u8 {
    if hut(w, p) {
        0
    } else if moon >= 0.9 {
        rng.next_int(VARIANTS.len() as u32) as u8
    } else {
        1 + rng.next_int(VARIANTS.len() as u32 - 1) as u8
    }
}
fn in_box(m: &Mob, p: IVec3, radius: f64) -> bool {
    let lo = p.as_dvec3() - DVec3::new(radius, 8.0, radius);
    let hi = p.as_dvec3() + DVec3::new(radius + 1.0, 9.0, radius + 1.0);
    let s = m.shape();
    let min = m.pos - DVec3::new(s.half_width, 0.0, s.half_width);
    let max = m.pos + DVec3::new(s.half_width, s.height, s.half_width);
    min.cmplt(hi).all() && max.cmpgt(lo).all()
}
fn placement<W: MobWorld + ?Sized>(w: &W, p: IVec3) -> bool {
    let Some(below) = w.block(p - IVec3::Y) else { return false };
    let Some(at) = w.block(p) else { return false };
    let Some(above) = w.block(p + IVec3::Y) else { return false };
    super::monster_spawns::valid_spawn_floor(below, MobKind::Cat)
        && super::monster_spawns::valid_empty_cell(at, MobKind::Cat)
        && super::monster_spawns::valid_empty_cell(above, MobKind::Cat)
}
/// The old hut marker was one local Z short. Preserve partial Peaceful saves
/// when correcting the cat's position (the witch marker fixes local +X).
pub(super) fn legacy_hut_marker(b: &Built, cat: IVec3) -> Option<IVec3> {
    if b.kind != Kind::SwampHut {
        return None;
    }
    let witch = b.residents.iter().find(|(k, _)| *k == MobKind::Witch)?.1;
    let right = witch - cat;
    (right.y == 0 && right.x.abs() + right.z.abs() == 1).then_some(cat - IVec3::new(-right.z, 0, right.x))
}
impl Entities {
    pub(super) fn cat_spawner_tick<W: MobWorld + ?Sized>(&mut self, dt: f64, w: &W, ctx: &Ctx) {
        if !ctx.spawning || ctx.dimension != Dimension::Overworld || dt <= 0.0 {
            return;
        }
        self.cat_spawner.seconds -= dt;
        if self.cat_spawner.seconds > 1e-9 {
            return;
        }
        self.cat_spawner.seconds = f64::from(DELAY_TICKS) / 20.0;
        // getRandomPlayer selects living players, including spectators.
        let count = ctx.players.iter().filter(|p| p.alive).count();
        if count == 0 {
            return;
        }
        let player = ctx.players.iter().filter(|p| p.alive).nth(self.rng.next_int(count as u32) as usize).unwrap();
        let mut offset = || (8 + self.rng.next_int(24) as i32) * if self.rng.next_int(2) == 0 { -1 } else { 1 };
        let p = player.pos.floor().as_ivec3() + IVec3::new(offset(), 0, offset());
        self.try_cat_spawn(w, p);
    }
    fn occupied_pois<W: MobWorld + ?Sized>(&self, w: &W) -> (Vec<IVec3>, Vec<IVec3>) {
        let claimed: FxHashSet<_> = self
            .mobs
            .iter()
            .chain(self.storage.mobs())
            .filter(|m| m.kind == MobKind::Villager && m.alive())
            .filter_map(|m| m.villager.as_ref()?.home)
            .collect();
        let jobs: FxHashSet<_> = self
            .mobs
            .iter()
            .chain(self.storage.mobs())
            .filter(|m| m.kind == MobKind::Villager && m.alive())
            .filter_map(|m| m.villager.as_ref()?.job)
            .collect();
        let mut homes = Vec::new();
        let mut centers = Vec::new();
        w.village_pois(&mut |p, b| {
            if b.is_bed_head() && claimed.contains(&p) {
                homes.push(p);
                centers.push(p);
            }
            if jobs.contains(&p) {
                centers.push(p);
            }
        });
        homes.sort_unstable_by_key(|p| p.to_array());
        homes.dedup();
        (homes, centers)
    }
    pub(super) fn try_cat_spawn<W: MobWorld + ?Sized>(&mut self, w: &W, p: IVec3) -> bool {
        if ![-10, 10].into_iter().all(|x| [-10, 10].into_iter().all(|z| w.loaded(p + IVec3::new(x, 0, z))))
            || !placement(w, p)
        {
            return false;
        }
        let (homes, centers) = self.occupied_pois(w);
        let section = p.div_euclid(IVec3::splat(16));
        let near_village = centers.iter().any(|h| (h.div_euclid(IVec3::splat(16)) - section).abs().max_element() <= 2);
        // Village cats despawn like other creatures; a swamp hut's cat persists.
        let persistent = !near_village;
        if near_village {
            if homes.iter().filter(|h| h.as_dvec3().distance_squared(p.as_dvec3()) <= 48.0 * 48.0).count() < MIN_BEDS
                || self.mobs.iter().filter(|m| m.kind == MobKind::Cat && in_box(m, p, 48.0)).count() >= VILLAGE_CAP
            {
                return false;
            }
        } else if !hut(w, p) || self.mobs.iter().any(|m| m.kind == MobKind::Cat && in_box(m, p, 16.0)) {
            return false;
        }
        // The pinned CatSpawner finalizes before snapTo: its variant SpawnContext
        // is at the new entity's origin, unlike SwampHutPiece's resident cat.
        let color = variant(w, IVec3::ZERO, self.moon_brightness, &mut self.rng);
        self.spawn(MobKind::Cat, p.as_dvec3() + DVec3::new(0.5, 0.0, 0.5));
        let m = self.mobs.last_mut().unwrap();
        m.yaw = 0.0;
        m.persistent = persistent;
        m.animal.as_mut().unwrap().variant = color;
        true
    }
}

#[cfg(test)]
mod tests;
