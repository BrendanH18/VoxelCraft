//! The overworld, the Nether and the End: lighting portals, travelling
//! through them, and keeping each dimension's blocks, containers and
//! dropped items in its own save.
//!
//! Saves keep the overworld where they always have (the world folder), the
//! Nether in a `nether` folder inside it and the End in `end`. The root
//! `level.txt` holds the player and everything global, plus the overworld's
//! furnaces, chests and items; `nether/level.txt` holds the Nether's.

use std::collections::BTreeMap;
use std::sync::Arc;

use glam::{DVec3, IVec3};
use rustc_hash::FxHashMap;

use crate::audio::sounds::{Material, Sound};
use crate::item::Item;
use crate::world::World;
use crate::world::block::Block;
use crate::world::chunk::ChunkData;
use crate::world::storage::Storage;
use crate::world::terrain::{Dimension, Generator, SEA_LEVEL};

use super::Game;

/// Level properties that belong to one dimension rather than the player.
pub(super) const DIMENSION_KEYS: [&str; 7] = ["furnaces", "chests", "items", "orbs", "spawners", "brewing", "dragon"];
/// Seconds of standing in a portal before it takes you (creative: almost
/// at once).
const PORTAL_TIME: f32 = 4.0;
const CREATIVE_PORTAL_TIME: f32 = 0.5;
/// How far around the arrival point to look for an existing portal.
const SEARCH_RADIUS: i32 = 16;
/// Steady light level of the sky-less Nether (the shader's daylight).
pub(super) const NETHER_LIGHT: f32 = 0.42;
pub(super) const NETHER_FOG: [f32; 3] = [0.20, 0.035, 0.025];

/// Where the player is headed while the destination streams in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Arrival {
    /// Through a portal, to the nearest portal around this cell (one is
    /// built if there's none).
    Portal(IVec3),
    /// Back from the dead: the bed or world spawn.
    Respawn,
    EndSpawn,
    /// Through an End gateway at `from` to the one at `exit`, which is
    /// built (with an island under it if `island`) and linked back if it's
    /// new.
    Gateway {
        exit: IVec3,
        from: IVec3,
        island: bool,
        link: bool,
    },
}

/// Heights a portal may stand at in a dimension.
fn portal_range(dim: Dimension) -> (i32, i32) {
    match dim {
        Dimension::Nether => (crate::world::nether::LAVA_SEA + 3, 116),
        Dimension::Overworld | Dimension::End => (2, 240),
    }
}

/// Where a dimension's save lives inside the world folder.
pub(super) fn storage_for(root: &Storage, dim: Dimension) -> Storage {
    match dim {
        Dimension::Overworld => Storage::new(root.dir()),
        Dimension::Nether => Storage::new(root.dir().join("nether")),
        Dimension::End => Storage::new(root.dir().join("end")),
    }
}

/// A dimension's saved chunks and properties. The overworld's properties
/// live in the root level file, which the caller already has.
pub(super) fn load_dimension(
    root: &Storage,
    dim: Dimension,
    overworld_props: &BTreeMap<String, String>,
) -> (FxHashMap<IVec3, Arc<ChunkData>>, BTreeMap<String, String>) {
    let storage = storage_for(root, dim);
    if !storage.exists() {
        return Default::default();
    }
    let chunks = storage.load_chunks().unwrap_or_else(|e| {
        log::error!("failed to load {} chunks: {e}", dim.name());
        Default::default()
    });
    let props = match dim {
        Dimension::Overworld => overworld_props.clone(),
        Dimension::Nether | Dimension::End => storage.load_level().map(|l| l.props).unwrap_or_default(),
    };
    (chunks, props)
}

/// The furnace, chest and item properties out of a level's properties.
pub(super) fn dimension_props(props: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    props.iter().filter(|(k, _)| DIMENSION_KEYS.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

impl Game {
    /// This dimension's furnaces, chests and dropped items, for saving.
    pub(super) fn dimension_props(&self) -> BTreeMap<String, String> {
        let mut props = BTreeMap::new();
        props.insert("furnaces".to_string(), self.world.furnaces_to_string());
        props.insert("brewing".to_string(), self.world.brewing_stands_to_string());
        props.insert("chests".to_string(), self.world.chests_to_string());
        props.insert("spawners".to_string(), self.world.spawners_to_string());
        props.insert("items".to_string(), self.mobs.entities.items_to_string());
        props.insert("orbs".to_string(), self.mobs.entities.orbs_to_string());
        if let Some(fight) = &self.mobs.entities.fight {
            props.insert("dragon".to_string(), fight.serialize());
        }
        props
    }

    /// Sets up a freshly loaded dimension's containers and items.
    pub(super) fn restore_dimension(&mut self, props: &BTreeMap<String, String>) {
        if let Some(b) = props.get("brewing") {
            self.world.load_brewing_stands(b);
        }
        if let Some(f) = props.get("furnaces") {
            self.world.load_furnaces(f);
        }
        if let Some(c) = props.get("chests") {
            self.world.load_chests(c);
        }
        if let Some(s) = props.get("spawners") {
            self.world.load_spawners(s);
        }
        if let Some(items) = props.get("items") {
            self.mobs.entities.load_items(items);
        }
        if let Some(orbs) = props.get("orbs") {
            self.mobs.entities.load_orbs(orbs);
        }
        self.mobs.entities.fight = self
            .world
            .generator
            .end()
            .map(|end| crate::entity::dragon::Fight::load(end, props.get("dragon").map(String::as_str)));
    }

    /// Saves this dimension and swaps in `to`, with the player waiting at
    /// `arrival` until the ground there has loaded.
    pub(super) fn switch_dimension(&mut self, to: Dimension, arrival: Arrival) {
        // Scripted runs (screenshots, benchmarks) never write saves.
        if self.settings_path.is_some() {
            self.save();
        }
        if self.dimension == Dimension::Overworld {
            self.overworld_props = self.dimension_props();
        }
        let (chunks, props) = load_dimension(&self.storage, to, &self.overworld_props);
        let seed = self.world.generator.seed;
        log::info!("travelling to the {} ({} modified chunks)", to.name(), chunks.len());
        self.world = World::new(Arc::new(Generator::for_dimension(seed, to)), chunks, self.settings.render_distance);
        self.mobs.entities = crate::entity::Entities::new(seed);
        self.renderer.clear_world();
        self.particles.clear();
        self.dimension = to;
        self.restore_dimension(&props);
        self.arrival = Some(arrival);
        self.portal_time = 0.0;
        self.sleeping = None;
        self.actions.reset();
        self.player.vel = DVec3::ZERO;
        self.vitals.reset_fall();
    }

    /// Goes through the portal the player is standing in.
    fn travel(&mut self) {
        let to = self.dimension.other();
        let scale = if to == Dimension::Nether { 1.0 / 8.0 } else { 8.0 };
        let (x, z) = ((self.player.pos.x * scale).floor() as i32, (self.player.pos.z * scale).floor() as i32);
        let y = match to {
            Dimension::Nether => (self.player.pos.y as i32).clamp(40, 100),
            Dimension::End => unreachable!("Nether portals cannot enter the End"),
            Dimension::Overworld => {
                let ground = Generator::new(self.world.generator.seed).column(x, z).height;
                ground.max(SEA_LEVEL) + 1
            }
        };
        self.switch_dimension(to, Arrival::Portal(IVec3::new(x, y, z)));
    }

    /// While travelling: holds the player in place until the destination
    /// has streamed in, then puts them in (or builds) the portal there, or
    /// at their respawn point. Returns whether the player is still waiting.
    pub(super) fn update_arrival(&mut self) -> bool {
        let Some(arrival) = self.arrival else { return false };
        let target = match arrival {
            Arrival::Portal(p) => p,
            Arrival::EndSpawn => crate::world::end::SPAWN,
            Arrival::Gateway { exit, .. } => exit,
            Arrival::Respawn => {
                self.spawn_bed.unwrap_or_else(|| self.world.generator.find_spawn() + IVec3::Y * (SEARCH_RADIUS + 8))
            }
        };
        let r = match arrival {
            Arrival::Portal(_) => SEARCH_RADIUS,
            Arrival::Respawn => 0,
            Arrival::EndSpawn => 2,
            Arrival::Gateway { .. } => 6,
        };
        let ready = [(-r, -r), (r, -r), (-r, r), (r, r), (0, 0)]
            .iter()
            .all(|&(dx, dz)| self.world.column_loaded(target.x + dx, target.z + dz));
        if !ready {
            self.player.pos = target.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
            self.player.vel = DVec3::ZERO;
            return true;
        }
        self.player.pos = match arrival {
            Arrival::Portal(p) => {
                let range = portal_range(self.dimension);
                let at = self.world.find_portal(p, SEARCH_RADIUS, range);
                let at = at.unwrap_or_else(|| self.world.build_portal(p, range));
                // Arriving in a portal doesn't send you straight back.
                self.portal_locked = true;
                // Face out of the portal's open side.
                let along_x = [IVec3::X, IVec3::NEG_X]
                    .iter()
                    .any(|&d| matches!(self.world.get_block(at + d), Some(Block::NETHER_PORTAL | Block::OBSIDIAN)));
                self.player.yaw = if along_x { std::f32::consts::FRAC_PI_2 } else { 0.0 };
                self.player.pitch = 0.0;
                at.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)
            }
            Arrival::EndSpawn => {
                let spawn = crate::world::end::SPAWN;
                for z in -2..=2 {
                    for x in -2..=2 {
                        self.world.set_block(spawn + IVec3::new(x, -1, z), Block::OBSIDIAN);
                        for y in 0..3 {
                            self.world.set_block(spawn + IVec3::new(x, y, z), Block::AIR);
                        }
                    }
                }
                self.player.yaw = std::f32::consts::PI;
                self.player.pitch = 0.0;
                spawn.as_dvec3() + DVec3::new(0.5, 0.0, 0.5)
            }
            Arrival::Respawn => self.respawn_point(),
            Arrival::Gateway { exit, from, island, link } => {
                if island && self.world.get_block(exit - IVec3::Y * 10) == Some(Block::AIR) {
                    self.world.build_end_island(exit - IVec3::Y * 10);
                }
                if self.world.get_block(exit) != Some(Block::END_GATEWAY) {
                    self.world.build_gateway(exit);
                }
                if link && let Some(fight) = &mut self.mobs.entities.fight {
                    fight.link_gateways(from, exit);
                    fight.link_gateways(exit, from);
                }
                self.audio.play(Sound::Teleport, None, 0.8, (0.9, 1.0));
                self.world.gateway_arrival(exit)
            }
        };
        self.relocate_agents();
        self.player.vel = DVec3::ZERO;
        self.player.flying = self.player.flying && self.mode.can_fly();
        self.vitals.reset_fall();
        self.arrival = None;
        let sound = Sound::Place(Material::Glass);
        self.audio.play(sound, None, 0.8, (0.5, 0.6));
        false
    }

    /// A cell of `block` the player overlaps.
    fn touched(&self, block: Block) -> Option<IVec3> {
        let (min, max) = crate::player::SHAPE.aabb(self.player.pos);
        let (lo, hi) = (min.floor().as_ivec3(), (max - DVec3::splat(1e-6)).floor().as_ivec3());
        (lo.y..=hi.y)
            .flat_map(|y| (lo.z..=hi.z).flat_map(move |z| (lo.x..=hi.x).map(move |x| IVec3::new(x, y, z))))
            .find(|&p| self.world.get_block(p) == Some(block))
    }

    /// Takes the player through the End gateway at `cell`: to its known
    /// exit, or out to the outer islands the first time.
    pub(super) fn enter_gateway(&mut self, cell: IVec3) {
        let Some(fight) = &self.mobs.entities.fight else { return };
        let arrival = match fight.gateway_exit(cell) {
            Some(exit) => Arrival::Gateway { exit, from: cell, island: false, link: false },
            None => {
                let Some(end) = self.world.generator.end() else { return };
                let (exit, island) = end.gateway_exit(cell);
                Arrival::Gateway { exit, from: cell, island, link: true }
            }
        };
        self.audio.play(Sound::Teleport, None, 0.8, (0.7, 0.8));
        self.arrival = Some(arrival);
        self.player.vel = DVec3::ZERO;
        self.vitals.reset_fall();
    }

    /// Whether the player overlaps a `portal` block's slice from `bottom`
    /// to `top` of the way up its cell.
    fn in_portal(&self, portal: Block, (bottom, top): (f64, f64)) -> bool {
        let (min, max) = crate::player::SHAPE.aabb(self.player.pos);
        let (lo, hi) = (min.floor().as_ivec3(), (max - DVec3::splat(1e-6)).floor().as_ivec3());
        (lo.y..=hi.y).any(|y| {
            min.y < y as f64 + top
                && max.y > y as f64 + bottom
                && (lo.z..=hi.z)
                    .any(|z| (lo.x..=hi.x).any(|x| self.world.get_block(IVec3::new(x, y, z)) == Some(portal)))
        })
    }

    /// Standing in a Nether portal long enough takes you to the other
    /// dimension. Falling into an End portal takes you at once: to the End's
    /// obsidian platform, or out of the End to your spawn point.
    pub(super) fn update_portal(&mut self, dt: f64) {
        if self.dimension == Dimension::End
            && !self.vitals.is_dead()
            && let Some(cell) = self.touched(Block::END_GATEWAY)
        {
            self.enter_gateway(cell);
            return;
        }
        // Java's End portal block spans 6..12 sixteenths: coming up from
        // below takes you only once you reach it.
        if !self.vitals.is_dead() && self.in_portal(Block::END_PORTAL, (6.0 / 16.0, 0.75)) {
            let (to, arrival) = match self.dimension {
                Dimension::End => (Dimension::Overworld, Arrival::Respawn),
                _ => (Dimension::End, Arrival::EndSpawn),
            };
            self.switch_dimension(to, arrival);
            return;
        }
        if self.dimension == Dimension::End
            || !self.in_portal(Block::NETHER_PORTAL, (0.0, 1.0))
            || self.vitals.is_dead()
        {
            self.portal_locked = false;
            self.portal_time = (self.portal_time - dt as f32 * 2.0).max(0.0);
            return;
        }
        if self.portal_locked {
            return;
        }
        let before = self.portal_time;
        self.portal_time += dt as f32;
        if before == 0.0 {
            self.audio.play(Sound::Fuse, None, 0.4, (0.4, 0.5));
        }
        if self.portal_time >= self.portal_needed() {
            self.travel();
        }
    }

    pub(super) fn portal_needed(&self) -> f32 {
        if self.mode.is_creative() { CREATIVE_PORTAL_TIME } else { PORTAL_TIME }
    }

    /// Right-click with flint and steel: primes TNT or lights a portal/fire
    /// in front of the clicked face. Only successful uses spend durability.
    pub(super) fn strike_flint(&mut self, pos: IVec3, normal: IVec3) -> bool {
        if self.held_item() != Some(Item::FLINT_AND_STEEL) {
            return false;
        }
        if self.world.get_block(pos) == Some(Block::TNT) {
            self.world.set_block(pos, Block::AIR);
            self.mobs.entities.prime_tnt(pos, false);
            self.audio.play(Sound::Fuse, Some(pos.as_dvec3()), 1.0, (0.95, 1.05));
            if self.mode.is_survival() && self.inventory.wear(self.actions.selected, 1) {
                self.show_popup("Flint and steel broke");
            }
            return true;
        }
        let at = pos + normal;
        if !self.world.ignite(at) {
            return true;
        }
        self.audio.play(Sound::Place(Material::Stone), Some(pos.as_dvec3()), 0.7, (1.6, 1.9));
        if self.mode.is_survival() && self.inventory.wear(self.actions.selected, 1) {
            self.show_popup("Flint and steel broke");
        }
        if self.world.get_block(at) == Some(Block::NETHER_PORTAL) {
            self.audio.play(Sound::Fuse, Some(at.as_dvec3()), 1.0, (0.5, 0.6));
        }
        true
    }

    /// Beds explode in both the Nether and the End.
    pub(super) fn bed_explodes(&mut self, pos: IVec3) -> bool {
        if self.dimension.has_sky() {
            return false;
        }
        let half = self.world.get_block(pos).unwrap_or(Block::AIR);
        self.world.set_block(pos, Block::AIR);
        self.break_bed_partner(pos, half);
        self.explode(pos.as_dvec3() + DVec3::splat(0.5), 5.0, "was killed by [Intentional Game Design]");
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::storage::LevelInfo;

    #[test]
    fn each_dimension_keeps_its_own_save() {
        let dir = std::env::temp_dir().join(format!("voxelcraft-dims-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = Storage::new(&dir);
        let props = |pairs: &[(&str, &str)]| -> BTreeMap<String, String> {
            pairs.iter().map(|&(k, v)| (k.to_string(), v.to_string())).collect()
        };
        let chunk = |b| (IVec3::new(1, 2, 3), Arc::new(ChunkData::Uniform(b)));

        // The overworld lives in the root folder, its furnaces with the player.
        let root_props = props(&[("mode", "survival"), ("chests", "overworld chests"), ("dimension", "nether")]);
        let level = LevelInfo { seed: 9, player: None, props: root_props.clone() };
        root.save(&level, &[chunk(Block::STONE)]).unwrap();
        let nether = storage_for(&root, Dimension::Nether);
        let level = LevelInfo { seed: 9, player: None, props: props(&[("chests", "nether chests")]) };
        nether.save(&level, &[chunk(Block::NETHERRACK)]).unwrap();

        let overworld_props = dimension_props(&root_props);
        assert_eq!(overworld_props, props(&[("chests", "overworld chests")]));
        let (chunks, p) = load_dimension(&root, Dimension::Overworld, &overworld_props);
        assert_eq!((chunks.len(), p.get("chests").map(String::as_str)), (1, Some("overworld chests")));
        assert!(matches!(*chunks[&IVec3::new(1, 2, 3)], ChunkData::Uniform(Block::STONE)));
        let (chunks, p) = load_dimension(&root, Dimension::Nether, &overworld_props);
        assert_eq!(p.get("chests").map(String::as_str), Some("nether chests"));
        assert!(matches!(*chunks[&IVec3::new(1, 2, 3)], ChunkData::Uniform(Block::NETHERRACK)));

        let end = storage_for(&root, Dimension::End);
        let level = LevelInfo { seed: 9, player: None, props: props(&[("chests", "end chests")]) };
        end.save(&level, &[chunk(Block::END_STONE)]).unwrap();
        let (chunks, p) = load_dimension(&root, Dimension::End, &overworld_props);
        assert_eq!(p.get("chests").map(String::as_str), Some("end chests"));
        assert!(matches!(*chunks[&IVec3::new(1, 2, 3)], ChunkData::Uniform(Block::END_STONE)));
        let (_, p) = load_dimension(&root, Dimension::Nether, &overworld_props);
        assert_eq!(p.get("chests").map(String::as_str), Some("nether chests"));

        // Rewriting just the root level leaves the overworld's chunks alone.
        root.save_level(&LevelInfo { seed: 9, player: None, props: root_props }).unwrap();
        assert_eq!(root.load_chunks().unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
