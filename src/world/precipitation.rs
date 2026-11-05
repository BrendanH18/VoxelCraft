//! Java 26.3 `ServerLevel.tickChunk/tickPrecipitation` snow accumulation.
use super::{World, block::Block, overworld_blocks as snow};
use glam::{IVec2, IVec3};

/// 26.3 uses an explicit block tag, not collision or light opacity. Resolve
/// the reference names once and use bounded state lookups on sampled columns.
fn motion_blocking(b: Block) -> bool {
    static STATES: std::sync::OnceLock<[bool; super::block::STATE_CAPACITY]> = std::sync::OnceLock::new();
    let states = STATES.get_or_init(|| {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/lightning_weather.json"))
                .expect("pinned weather tags");
        let names: rustc_hash::FxHashSet<&str> = data["motion_blocking"]
            .as_array()
            .expect("motion tag")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        std::array::from_fn(|i| names.contains(crate::content::mining::reference_name(Block(i as u16).base())))
    });
    b.holds_water() || b.is_lava() || states.get(usize::from(b.0)).copied().unwrap_or(false)
}

impl World {
    /// Loaded block-ticking Java chunk columns, once each despite overlapping players.
    pub(crate) fn weather_chunks(&self) -> Vec<IVec2> {
        let mut chunks = Vec::new();
        let keys = match self.activity.block_columns() {
            Some(bounds) => {
                let mut keys = Vec::new();
                for (lo, hi) in bounds {
                    for x in lo.x..=hi.x {
                        for z in lo.y..=hi.y {
                            keys.push(IVec2::new(x, z));
                        }
                    }
                }
                keys.sort_unstable_by_key(|c| (c.x, c.y));
                keys.dedup();
                keys
            }
            None => self.columns.keys().copied().collect(),
        };
        for key in keys {
            if self.columns.get(&key).is_none_or(|col| col.loaded != self.column_chunks()) {
                continue;
            }
            for x in 0..2 {
                for z in 0..2 {
                    let c = key * 2 + IVec2::new(x, z);
                    if self.activity.contains(IVec3::new(c.x * 16, 0, c.y * 16), true) {
                        chunks.push(c);
                    }
                }
            }
        }
        chunks.sort_unstable_by_key(|c| (c.x, c.y));
        chunks
    }

    /// MOTION_BLOCKING for the sampled weather column; bounded by the build
    /// height, and only queried after a rare weather roll succeeds.
    pub fn weather_surface(&self, x: i32, z: i32) -> Option<i32> {
        let top = self.world_surface_height(x, z)?;
        (self.min_y()..=top).rev().find(|&y| self.get_block(IVec3::new(x, y, z)).is_some_and(motion_blocking))
    }
    /// One attempt per random tick at 1/48 odds per Java 16×16 chunk.
    pub fn tick_precipitation(&mut self, random_tick_speed: u32) {
        self.snow_pushes.clear();
        if !self.raining || !self.generator.dimension.has_sky() || self.rules.max_snow_accumulation_height == 0 {
            return;
        }
        for c in self.weather_chunks() {
            for _ in 0..random_tick_speed {
                if !self.roll().is_multiple_of(48) {
                    continue;
                }
                let r = self.roll();
                self.accumulate_snow(c.x * 16 + (r & 15) as i32, c.y * 16 + (r >> 4 & 15) as i32);
            }
        }
    }

    pub fn take_snow_pushes(&mut self) -> Vec<(IVec3, f64)> {
        std::mem::take(&mut self.snow_pushes)
    }
    /// Heightmap-selected column. Java 26.3 excludes the snow block (every
    /// layer count) from blocks_motion_in_heightmap, including eight layers.
    pub fn accumulate_snow(&mut self, x: i32, z: i32) -> bool {
        if !self.raining || !self.generator.dimension.has_sky() {
            return false;
        }
        let cap = self.rules.max_snow_accumulation_height.min(8);
        let Some(h) = self.weather_surface(x, z) else { return false };
        let p = IVec3::new(x, h + 1, z);
        if cap == 0
            || self.snow_line_at(x, z).is_none_or(|line| line == i16::MAX || p.y < i32::from(line))
            || self.block_light(p) >= 10
        {
            return false;
        }
        let Some(b) = self.get_block(p) else { return false };
        let next = if let Some(n) = snow::snow_layers(b) {
            if n >= cap {
                return false;
            }
            snow::snow_layer(n + 1)
        } else {
            if b != Block::AIR
                || snow::can_stay_on(snow::snow_layer(1), self.get_block(p - IVec3::Y).unwrap_or(Block::AIR))
                    != Some(true)
            {
                return false;
            }
            snow::snow_layer(1)
        };
        if self.set_block(p, next) {
            if let Some(n) = snow::snow_layers(next).filter(|n| *n > 1) {
                self.snow_pushes.push((p, f64::from(n - 1) / 8.));
            }
            true
        } else {
            false
        }
    }
}

/// `Block.pushEntitiesUp` for the added snow collision slice.
pub fn push_entity(pos: &mut glam::DVec3, shape: crate::physics::Shape, cell: IVec3, height: f64) {
    let (lo, hi) = shape.aabb(*pos);
    let bottom = cell.as_dvec3() + glam::DVec3::Y * (height - 0.125);
    let top = cell.as_dvec3() + glam::DVec3::new(1., height, 1.);
    if hi.cmpgt(bottom).all() && lo.cmplt(top).all() {
        pos.y = top.y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{
        chunk::{CHUNK_SIZE, ChunkData},
        terrain::Generator,
    };
    use std::sync::Arc;
    fn world() -> World {
        let mut w = World::new_headless(Arc::new(Generator::new(263)), Default::default(), 2);
        for y in w.generator.dimension.chunk_rows() {
            w.insert_chunk(IVec3::new(0, y, 0), Arc::new(ChunkData::Uniform(Block::AIR)), false);
        }
        w.columns.get_mut(&IVec2::ZERO).unwrap().snow = Some(Box::new([i16::MIN; CHUNK_SIZE * CHUNK_SIZE]));
        w.raining = true;
        w.set_block(IVec3::new(8, 140, 8), Block::STONE);
        w
    }
    #[test]
    fn snowfall_obeys_default_zero_and_eight_and_persists_as_existing_states() {
        let mut w = world();
        let p = IVec3::new(8, 141, 8);
        assert!(w.accumulate_snow(8, 8));
        assert_eq!(w.get_block(p), Some(snow::snow_layer(1)));
        assert!(!w.accumulate_snow(8, 8));
        w.rules.max_snow_accumulation_height = 0;
        assert!(!w.accumulate_snow(8, 8));
        w.rules.max_snow_accumulation_height = 8;
        for n in 2..=8 {
            assert!(w.accumulate_snow(8, 8));
            assert_eq!(w.get_block(p), Some(snow::snow_layer(n)));
        }
        // Java 26.3 blocks_motion_in_heightmap excludes snow at every layer.
        assert!(!w.accumulate_snow(8, 8));
        assert_eq!(w.get_block(p + IVec3::Y), Some(Block::AIR));
        let data = w.chunks[&IVec3::new(0, 4, 0)].data.clone();
        w.insert_chunk(IVec3::new(0, 4, 0), data, true);
        assert_eq!(w.get_block(p), Some(snow::snow_layer(8)));
    }
    #[test]
    fn rain_dry_biomes_light_and_roofs_prevent_snow() {
        let mut w = world();
        w.raining = false;
        assert!(!w.accumulate_snow(8, 8));
        w.raining = true;
        w.columns.get_mut(&IVec2::ZERO).unwrap().snow = Some(Box::new([i16::MAX; CHUNK_SIZE * CHUNK_SIZE]));
        assert!(!w.accumulate_snow(8, 8));
        w.columns.get_mut(&IVec2::ZERO).unwrap().snow = Some(Box::new([i16::MIN; CHUNK_SIZE * CHUNK_SIZE]));
        w.set_block(IVec3::new(8, 140, 9), Block::STONE);
        w.set_block(IVec3::new(8, 141, 9), Block::TORCH);
        w.update_block_light();
        assert!(!w.accumulate_snow(8, 8));
    }
    #[test]
    fn zero_random_ticks_stops_natural_accumulation() {
        let mut w = world();
        for _ in 0..100 {
            w.tick_precipitation(0);
        }
        assert_eq!(w.get_block(IVec3::new(8, 141, 8)), Some(Block::AIR));
    }
    #[test]
    fn added_collision_lifts_players_without_moving_distant_bodies() {
        let p = IVec3::new(8, 141, 8);
        let mut pos = glam::DVec3::new(8.5, 141., 8.5);
        push_entity(&mut pos, crate::player::SHAPE, p, 0.125);
        assert_eq!(pos.y, 141.125);
        let mut far = glam::DVec3::new(10.5, 141., 8.5);
        push_entity(&mut far, crate::player::SHAPE, p, 0.125);
        assert_eq!(far.y, 141.);
    }
}
